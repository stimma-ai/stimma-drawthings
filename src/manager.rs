//! Embedded provider manager, shared by Stimma's popover and a normal browser.
use crate::{catalog, install, provider::App, store};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::{atomic::Ordering, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, Mutex};

pub const PREFIX: &str = "/stp-v1/manage/";
#[derive(Default)]
pub struct Manager {
    pub metrics: crate::metrics::Metrics,
    pub activity: Mutex<VecDeque<Value>>,
    pub operation: Mutex<Option<(String, tokio::task::AbortHandle)>>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl Manager {
    pub async fn begin(&self, id: &str, title: &str) {
        let mut history = self.activity.lock().await;
        history.push_front(json!({"id":id,"title":title,"state":"running","detail":"Starting","progress":0,"started_at":now()}));
        history.truncate(50);
    }
    pub async fn observe(&self, prefix: &str, message: &Value) {
        let p = &message["params"];
        let Some(request) = p["request_id"].as_str() else {
            return;
        };
        let id = format!("{prefix}{request}");
        let mut history = self.activity.lock().await;
        let Some(row) = history.iter_mut().find(|row| row["id"] == id) else {
            return;
        };
        match message["method"].as_str() {
            Some("tools.progress") => {
                row["detail"] = p["status"].clone();
                row["progress"] = p["progress"].clone();
            }
            Some("tools.result") => {
                row["state"] = json!(if p["success"] == true {
                    "done"
                } else if p["error"]["code"] == "CANCELLED" {
                    "cancelled"
                } else {
                    "failed"
                });
                row["detail"] = if p["success"] == true {
                    json!("Complete")
                } else {
                    p["error"]["message"].clone()
                };
                row["finished_at"] = json!(now());
            }
            _ => {}
        }
    }
    async fn finish(&self, id: &str, result: &anyhow::Result<()>) {
        let mut history = self.activity.lock().await;
        if let Some(row) = history.iter_mut().find(|row| row["id"] == id) {
            row["state"] = json!(if result.is_ok() { "done" } else { "failed" });
            row["detail"] = json!(match result {
                Ok(()) => "Complete".to_owned(),
                Err(e) => format!("{e:#}"),
            });
            row["finished_at"] = json!(now());
        }
    }
}

pub fn routes(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(|| async { Redirect::temporary(PREFIX) }))
        .route(
            "/stp-v1/manage",
            get(|| async { Redirect::permanent(PREFIX) }),
        )
        .route(PREFIX, get(index))
        .route("/stp-v1/manage/api/overview", get(overview))
        .route("/stp-v1/manage/api/action", post(action))
        .route("/stp-v1/manage/{*path}", get(asset))
        .with_state(app)
}
include!(concat!(env!("OUT_DIR"), "/manager_assets.rs"));
async fn index() -> Response {
    static_response("index.html")
}
async fn asset(Path(path): Path<String>) -> Response {
    static_response(&path)
}
fn static_response(path: &str) -> Response {
    match embedded_asset(path) {
        Some((mime, bytes)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn overview(State(app): State<Arc<App>>, headers: HeaderMap) -> Json<Value> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:8765");
    let stp_url = format!("ws://{host}/stp-v1");
    let online = tokio::time::timeout(Duration::from_secs(2), app.runtime.engine.catalog())
        .await
        .is_ok_and(|r| r.is_ok());
    let meta = app.catalog.read().await;
    let models = store::array(&meta.models);
    let dir = app.runtime.models.clone().or_else(|| {
        if app.runtime.managed {
            install::desktop_models().ok()
        } else {
            None
        }
    });
    let profiles: Vec<_> = catalog::profiles(&meta).iter().map(|profile| {
        let files: Vec<_> = profile.files.iter().map(|file| {
            let m = models.iter().find(|m| m["file"] == *file).unwrap();
            let installed = if app.runtime.managed { dir.as_ref().is_some_and(|d| d.join(file).is_file()) } else { m["stp_installed"] == true };
            json!({"file":file,"name":m["name"],"installed":installed,"size_bytes":dir.as_ref().and_then(|d| std::fs::metadata(d.join(file)).ok()).map(|m|m.len())})
        }).collect();
        json!({"id":profile.id,"name":profile.name,"kind":if profile.video {"video"} else {"image"},"files":files})
    }).collect();
    let local = app.runtime.managed
        || reqwest::Url::parse(&app.runtime.engine.endpoint)
            .ok()
            .is_some_and(|url| matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")));
    let pid = app
        .runtime
        .child
        .try_lock()
        .ok()
        .and_then(|child| child.as_ref().and_then(|c| c.id()));
    let metrics = if local {
        app.manager.metrics.sample(pid).await
    } else {
        Value::Null
    };
    let activity = app.manager.activity.lock().await.clone();
    let operation = app
        .manager
        .operation
        .lock()
        .await
        .as_ref()
        .filter(|(_, task)| !task.is_finished())
        .map(|(id, _)| id.clone());
    Json(
        json!({"metrics":metrics,"stp_url":stp_url,"version":env!("CARGO_PKG_VERSION"),"engine_online":online,"managed":app.runtime.managed,"busy":app.capacity.available_permits()==0,"jobs":app.jobs.load(Ordering::SeqCst),"offline":app.runtime.offline,"models_dir":dir.map(|d| d.to_string_lossy().into_owned()),"engine_endpoint":app.runtime.engine.endpoint,"profiles":profiles,"tools_count":catalog::descriptors(&meta)["tools"].as_array().unwrap().len(),"loras_count":store::array(&meta.loras).len(),"activity":activity,"operation":operation}),
    )
}
fn failure(code: StatusCode, text: &str) -> Response {
    (code, Json(json!({"error":text}))).into_response()
}
async fn action(State(app): State<Arc<App>>, Json(body): Json<Value>) -> Response {
    let kind = body["action"].as_str().unwrap_or("").to_owned();
    if kind == "cancel" {
        let operation = app.manager.operation.lock().await.take();
        let Some((id, task)) = operation.filter(|(_, task)| !task.is_finished()) else {
            return failure(StatusCode::CONFLICT, "No manager operation is running");
        };
        task.abort();
        app.manager.observe("", &json!({"method":"tools.result","params":{"request_id":id,"success":false,"error":{"code":"CANCELLED","message":"Cancelled"}}})).await;
        let _ = app.events.send(json!({"jsonrpc":"2.0","method":"provider.state","params":{"state":"ready","summary":"Cancelled"}}));
        return Json(json!({"cancelled":true})).into_response();
    }

    if !["start", "stop", "refresh", "install"].contains(&kind.as_str()) {
        return failure(StatusCode::BAD_REQUEST, "Unknown action");
    }
    if kind == "stop" && !app.runtime.managed {
        return failure(
            StatusCode::CONFLICT,
            "This engine is managed by Draw Things. Stop it there.",
        );
    }
    let file = body["file"].as_str().unwrap_or("").to_owned();
    if kind == "install" {
        let meta = app.catalog.read().await;
        if !catalog::profiles(&meta)
            .iter()
            .any(|p| p.files.contains(&file))
        {
            return failure(
                StatusCode::BAD_REQUEST,
                "Choose a checkpoint from the tool catalog",
            );
        }
    }
    let Ok(permit) = app.capacity.clone().try_acquire_owned() else {
        return failure(StatusCode::CONFLICT, "Wait for the current task to finish");
    };
    let id = uuid::Uuid::new_v4().to_string();
    let title = match kind.as_str() {
        "start" => "Start engine",
        "stop" => "Stop engine",
        "refresh" => "Refresh models",
        _ => "Download model",
    };
    let title = if kind == "install" {
        let meta = app.catalog.read().await;
        let name = store::array(&meta.models)
            .iter()
            .find(|m| m["file"] == file)
            .and_then(|m| m["name"].as_str())
            .unwrap_or("model")
            .to_owned();
        format!("Download {name}")
    } else {
        title.to_owned()
    };
    app.manager.begin(&id, &title).await;
    if kind == "install" {
        if let Some(row) = app.manager.activity.lock().await.front_mut() {
            row["file"] = json!(file);
        }
    }
    let _ = app.events.send(json!({"jsonrpc":"2.0","method":"provider.state","params":{"state":"in_progress","summary":title}}));
    let response = (StatusCode::ACCEPTED, Json(json!({"id":id}))).into_response();
    let operation_id = id.clone();
    let task_app = app.clone();
    let task = tokio::spawn(async move {
        let app = task_app;
        let _permit = permit;
        let (tx, mut rx) = mpsc::channel(32);
        let progress = install::Progress { tx, id: id.clone() };
        let manager = app.manager.clone();
        let observer = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                manager.observe("", &message).await;
            }
        });
        let result: anyhow::Result<()> = async {
            match kind.as_str() {
                "start" => app.runtime.ensure(Some(&progress)).await?,
                "stop" => app.runtime.stop().await?,
                "refresh" => app.refresh(true).await?,
                "install" => {
                    app.runtime.ensure(Some(&progress)).await?;
                    let meta = app.catalog.read().await.clone();
                    store::ensure_model(&app.runtime, &meta, &file, &progress).await?;
                    if file.contains("_hne_") {
                        store::ensure_model(
                            &app.runtime,
                            &meta,
                            &file.replace("_hne_", "_lne_"),
                            &progress,
                        )
                        .await?;
                    }
                }
                _ => unreachable!(),
            }
            if kind != "refresh" {
                app.refresh(false).await?;
            }
            Ok(())
        }
        .await;
        drop(progress);
        let _ = observer.await;
        app.manager.finish(&id, &result).await;
        let _ = app.events.send(json!({"jsonrpc":"2.0","method":"provider.state","params":{"state":if result.is_ok() {"ready"} else {"error"},"summary":if result.is_ok() {"Ready"} else {"Open the manager for details"}}}));
        let _ = app
            .events
            .send(json!({"jsonrpc":"2.0","method":"tools.changed","params":{}}));
    });
    *app.manager.operation.lock().await = Some((operation_id, task.abort_handle()));
    response
}
