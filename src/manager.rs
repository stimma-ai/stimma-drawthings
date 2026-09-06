//! Embedded provider manager, shared by Stimma's popover and a normal browser.
//! The API mirrors the ComfyUI-Stimma manager: an overview, a tool list with
//! per-tool dependency plans, an activity log, and a few engine actions.
use crate::{catalog, install, provider::App, store};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, Mutex};

pub const PREFIX: &str = "/stp-v1/manage/";
const DOWNLOAD_BASE: &str = "https://static.libnnc.org/";
#[derive(Default)]
pub struct Manager {
    pub metrics: crate::metrics::Metrics,
    pub activity: Mutex<VecDeque<Value>>,
    pub operation: Mutex<Option<(String, tokio::task::AbortHandle)>>,
    /// Published component sizes, learned lazily from the download host.
    pub sizes: Mutex<HashMap<String, Option<u64>>>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl Manager {
    pub async fn begin(&self, id: &str, title: &str, kind: &str, tool: Option<&str>) {
        let mut history = self.activity.lock().await;
        history.push_front(json!({"id":id,"title":title,"kind":kind,"tool":tool,"state":"running","detail":"Starting","progress":0,"started_at":now()}));
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
    /// Content-Length of a published component, cached for the process lifetime.
    async fn size(&self, file: &str, offline: bool) -> Option<u64> {
        if let Some(size) = self.sizes.lock().await.get(file) {
            return *size;
        }
        if offline {
            return None;
        }
        let size = async {
            let mut url = reqwest::Url::parse(DOWNLOAD_BASE).ok()?;
            url.path_segments_mut().ok()?.push(file);
            let response = install::http()
                .ok()?
                .head(url)
                .timeout(Duration::from_secs(8))
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?;
            // HEAD bodies are empty, so read the advertised length header.
            response
                .headers()
                .get(header::CONTENT_LENGTH)?
                .to_str()
                .ok()?
                .parse::<u64>()
                .ok()
                .filter(|n| *n > 0)
        }
        .await;
        self.sizes.lock().await.insert(file.to_owned(), size);
        size
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
        .route("/stp-v1/manage/api/tools", get(tools))
        .route("/stp-v1/manage/api/tools/{id}", get(tool_detail))
        .route("/stp-v1/manage/api/tools/{id}/setup", post(tool_setup))
        .route("/stp-v1/manage/api/activity/clear-done", post(clear_done))
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

/// Where checkpoints live for a managed engine; None when attached remotely.
fn models_dir(app: &App) -> Option<PathBuf> {
    app.runtime.models.clone().or_else(|| {
        if app.runtime.managed {
            install::desktop_models().ok()
        } else {
            None
        }
    })
}
fn local_engine(app: &App) -> bool {
    app.runtime.managed
        || reqwest::Url::parse(&app.runtime.engine.endpoint)
            .ok()
            .is_some_and(|url| matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")))
}
fn engine_host(app: &App) -> String {
    reqwest::Url::parse(&app.runtime.engine.endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| app.runtime.engine.endpoint.clone())
}
async fn engine_online(app: &App) -> bool {
    tokio::time::timeout(Duration::from_secs(2), app.runtime.engine.catalog())
        .await
        .is_ok_and(|r| r.is_ok())
}
/// Presence of files: the model folder for a managed engine, else the engine.
async fn presence(app: &App, files: &[String]) -> HashMap<String, bool> {
    if let Some(dir) = models_dir(app) {
        return files
            .iter()
            .map(|f| (f.clone(), dir.join(f).is_file()))
            .collect();
    }
    app.runtime
        .engine
        .files_exist(files.to_vec())
        .await
        .unwrap_or_default()
}
fn local_size(app: &App, file: &str) -> Option<u64> {
    models_dir(app)
        .and_then(|d| std::fs::metadata(d.join(file)).ok())
        .map(|m| m.len())
}
fn disk(app: &App) -> Value {
    let dir = models_dir(app).unwrap_or_else(|| app.runtime.state.clone());
    match (fs2::available_space(&dir), fs2::total_space(&dir)) {
        (Ok(free), Ok(total)) => json!([{"path":dir.to_string_lossy(),"free":free,"total":total}]),
        _ => json!([]),
    }
}
fn task_label(task: &str) -> String {
    let mut label = task.replace('-', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}
/// Running download for a tool or file, if any.
fn active_download<'a>(
    activity: &'a VecDeque<Value>,
    profile: &catalog::Profile,
) -> Option<&'a Value> {
    activity.iter().find(|row| {
        row["state"] == "running"
            && row["kind"] == "download"
            && (row["tool"] == profile.id
                || row["file"]
                    .as_str()
                    .is_some_and(|f| profile.files.iter().any(|p| p == f)))
    })
}
fn tool_row(
    profile: &catalog::Profile,
    activity: &VecDeque<Value>,
    sizes: &HashMap<String, Option<u64>>,
) -> Value {
    let download = active_download(activity, profile);
    let failed = activity.iter().find(|row| {
        row["state"] == "failed" && row["kind"] == "download" && row["tool"] == profile.id
    });
    let ready = profile.ready();
    let summary = if let Some(row) = download {
        row["detail"].as_str().unwrap_or("Downloading").to_owned()
    } else if let Some(row) = failed {
        row["detail"]
            .as_str()
            .unwrap_or("Download failed")
            .to_owned()
    } else if ready {
        String::new()
    } else {
        match sizes.get(&profile.id).copied().flatten() {
            Some(n) => format!("{} download", fmt_bytes(n)),
            None => "Download required".to_owned(),
        }
    };
    json!({
        "id": profile.id,
        "name": profile.name,
        "kind": if profile.video { "video" } else { "image" },
        "task_types": profile.task_types(),
        "tasks": profile.task_types().iter().map(|t| task_label(t)).collect::<Vec<_>>(),
        "state": if ready { "ready" } else { "needs_setup" },
        "in_progress": download.is_some(),
        "failed": failed.is_some() && download.is_none(),
        "summary": summary,
        "recommended": profile.files[0],
        "installed": profile.installed,
        "variants": profile.files.len(),
    })
}
/// What a checkpoint's component is for, from the catalog entry that names it.
fn role(models: &[Value], checkpoint: &str, name: &str) -> &'static str {
    if name == checkpoint {
        return "Checkpoint";
    }
    if name.contains("_lne_") {
        return "Low-noise expert";
    }
    let Some(model) = models.iter().find(|m| m["file"] == checkpoint) else {
        return "Component";
    };
    let names = |key: &str| {
        model[key]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v == name || v["file"] == name)
    };
    if ["text_encoder", "clip_encoder", "t5_encoder"]
        .iter()
        .any(|k| model[*k] == name)
        || names("additional_clip_encoders")
        || name.contains("clip")
    {
        "Text encoder"
    } else if model["autoencoder"] == name || name.contains("vae") {
        "Autoencoder"
    } else if model["image_encoder"] == name {
        "Image encoder"
    } else if names("latents_upscalers") {
        "Latent upscaler"
    } else if names("stage_models") {
        "Stage model"
    } else if model["diffusion_mapping"] == name {
        "Diffusion mapping"
    } else {
        "Component"
    }
}
fn fmt_bytes(n: u64) -> String {
    let gb = n as f64 / 1024f64.powi(3);
    if gb >= 10.0 {
        format!("{gb:.0} GB")
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", n as f64 / 1024f64.powi(2))
    }
}

async fn overview(State(app): State<Arc<App>>, headers: HeaderMap) -> Json<Value> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:8765");
    let stp_url = format!("ws://{host}/stp-v1");
    let online = engine_online(&app).await;
    let meta = app.catalog.read().await;
    let profiles = catalog::profiles(&meta);
    let tools_ready = profiles.iter().filter(|p| p.ready()).count();
    let local = local_engine(&app);
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
    let running: Vec<Value> = activity
        .iter()
        .filter(|row| row["state"] == "running" && row["kind"] == "generation")
        .cloned()
        .collect();
    let jobs = app.jobs.load(Ordering::SeqCst);
    let active = activity
        .iter()
        .filter(|row| row["state"] == "running")
        .count();
    let attention = activity
        .iter()
        .filter(|row| row["state"] == "failed")
        .count();
    let busy = app.capacity.available_permits() == 0;
    let (state, summary) = if operation.is_some() {
        (
            "in_progress",
            activity
                .iter()
                .find(|row| row["state"] == "running" && row["kind"] != "generation")
                .and_then(|row| row["title"].as_str())
                .unwrap_or("Working")
                .to_owned(),
        )
    } else if !online && !app.runtime.managed {
        ("error", "Draw Things is not reachable".to_owned())
    } else if tools_ready == 0 {
        ("warning", "No models downloaded yet".to_owned())
    } else if metrics["memory_pressure"] == "critical" {
        ("warning", "Memory pressure is critical".to_owned())
    } else {
        ("ready", String::new())
    };
    let hosts = if local {
        json!([{
            "host":"local","local":true,"reachable":true,"checking":false,
            "hostname":metrics["gpu_name"].as_str().unwrap_or("This Mac"),
            "engine_online":online,"engine_managed":app.runtime.managed,
            "gpus":[{"index":0,"name":metrics["gpu_name"],"util":metrics["gpu_utilization"],"mem_used":metrics["memory_used_bytes"],"mem_total":metrics["memory_total_bytes"]}],
            "memory_pressure":metrics["memory_pressure"],"swap_used":metrics["swap_used_bytes"],"compressed":metrics["compressed_bytes"],"engine_rss":metrics["engine_rss_bytes"],"gpu_memory":metrics["gpu_memory_bytes"]
        }])
    } else {
        json!([{"host":engine_host(&app),"local":false,"reachable":online,"checking":false,"hostname":engine_host(&app),"engine_online":online,"engine_managed":false,"gpus":[]}])
    };
    Json(json!({
        "state":state,"summary":summary,"checking":false,
        "activity_active_count":active,"activity_attention_count":attention,
        "hosts":hosts,"running":running,"pending":jobs.saturating_sub(running.len()),
        "disk":disk(&app),
        "tools_ready":tools_ready,"tools_total":profiles.len(),
        "tools_count":catalog::descriptors(&meta)["tools"].as_array().unwrap().len(),
        "loras_count":store::array(&meta.loras).len(),
        "engine":{"online":online,"managed":app.runtime.managed,"endpoint":app.runtime.engine.endpoint,"busy":busy,"local":local},
        "engine_online":online,"managed":app.runtime.managed,"busy":busy,"jobs":jobs,
        "offline":app.runtime.offline,"models_dir":models_dir(&app).map(|d| d.to_string_lossy().into_owned()),
        "stp_url":stp_url,"version":env!("CARGO_PKG_VERSION"),
        "activity":activity,"operation":operation,"metrics":metrics
    }))
}
async fn tools(State(app): State<Arc<App>>) -> Json<Value> {
    let meta = app.catalog.read().await;
    let activity = app.manager.activity.lock().await.clone();
    let sizes = app.manager.sizes.lock().await.clone();
    // Row summaries show the whole missing download when every component size is known.
    let mut totals: HashMap<String, Option<u64>> = HashMap::new();
    for profile in catalog::profiles(&meta).iter().filter(|p| !p.ready()) {
        let required = requirements(&meta, &profile.files[0]);
        let present = presence(&app, &required).await;
        let missing: Vec<_> = required
            .iter()
            .filter(|f| present.get(*f) != Some(&true))
            .collect();
        let total = missing
            .iter()
            .map(|f| sizes.get(*f).copied().flatten())
            .sum::<Option<u64>>();
        totals.insert(profile.id.clone(), total);
    }
    let mut rows: Vec<Value> = catalog::profiles(&meta)
        .iter()
        .map(|p| tool_row(p, &activity, &totals))
        .collect();
    rows.sort_by_key(|r| {
        (
            r["state"] != "ready",
            r["name"].as_str().unwrap_or("").to_lowercase(),
        )
    });
    Json(json!({"tools":rows,"offline":app.runtime.offline}))
}
/// Everything a checkpoint needs on disk, in download order.
fn requirements(meta: &crate::proto::MetadataOverride, file: &str) -> Vec<String> {
    let models = store::array(&meta.models);
    let mut files = vec![];
    let mut targets = vec![file.to_owned()];
    if file.contains("_hne_") {
        targets.push(file.replace("_hne_", "_lne_"));
    }
    for target in targets {
        if let Some(model) = models.iter().find(|m| m["file"] == target) {
            for dep in store::dependencies(model).unwrap_or_default() {
                if !files.contains(&dep) {
                    files.push(dep);
                }
            }
        }
    }
    // Checkpoints first, then supporting components.
    files.sort_by_key(|f| (f != file && !f.contains("_lne_"), f.clone()));
    files
}
async fn tool_detail(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let meta = app.catalog.read().await.clone();
    let Some(profile) = catalog::profiles(&meta).into_iter().find(|p| p.id == id) else {
        return failure(StatusCode::NOT_FOUND, "Unknown tool");
    };
    let file = query
        .get("file")
        .filter(|f| profile.files.contains(*f))
        .cloned()
        .unwrap_or_else(|| profile.files[0].clone());
    let models = store::array(&meta.models);
    let required = requirements(&meta, &file);
    let present = presence(&app, &required).await;
    let mut files = vec![];
    let mut total_size = 0u64;
    let mut unknown = 0;
    for name in &required {
        let installed = present.get(name) == Some(&true);
        let size = if installed {
            local_size(&app, name).or(app.manager.size(name, true).await)
        } else {
            app.manager.size(name, app.runtime.offline).await
        };
        if !installed {
            match size {
                Some(n) => total_size += n,
                None => unknown += 1,
            }
        }
        let role = role(&models, &file, name);
        let display = models
            .iter()
            .find(|m| m["file"] == *name)
            .and_then(|m| m["name"].as_str())
            .unwrap_or(name)
            .to_owned();
        files.push(
            json!({"filename":name,"name":display,"role":role,"installed":installed,"size":size}),
        );
    }
    let variants: Vec<Value> = profile
        .files
        .iter()
        .map(|f| {
            let name = models
                .iter()
                .find(|m| m["file"] == *f)
                .and_then(|m| m["name"].as_str())
                .unwrap_or(f)
                .to_owned();
            json!({"filename":f,"name":name,"installed":profile.installed.contains(f),"size":local_size(&app, f),"selected":*f == file})
        })
        .collect();
    let activity = app.manager.activity.lock().await.clone();
    let totals = HashMap::from([(
        profile.id.clone(),
        Some(total_size).filter(|n| *n > 0 && unknown == 0),
    )]);
    let mut row = tool_row(&profile, &activity, &totals);
    row["file"] = json!(file);
    row["files"] = json!(files);
    row["variants"] = json!(variants);
    row["missing"] = json!(files.iter().filter(|f| f["installed"] != true).count());
    row["total_size"] = json!(total_size);
    row["size_unknown"] = json!(unknown);
    row["free_space"] = disk(&app)[0]["free"].clone();
    row["target"] = json!(if models_dir(&app).is_some() {
        "this Mac"
    } else {
        "the Draw Things engine"
    });
    row["blockers"] = json!(
        if app.runtime.offline && row["missing"].as_u64().unwrap_or(0) > 0 {
            vec![json!({"kind":"offline"})]
        } else {
            vec![]
        }
    );
    Json(row).into_response()
}
async fn tool_setup(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let meta = app.catalog.read().await.clone();
    let Some(profile) = catalog::profiles(&meta).into_iter().find(|p| p.id == id) else {
        return failure(StatusCode::NOT_FOUND, "Unknown tool");
    };
    let file = body["file"]
        .as_str()
        .filter(|f| profile.files.iter().any(|p| p == f))
        .unwrap_or(&profile.files[0])
        .to_owned();
    start(app, "install", file, Some(profile.id)).await
}
async fn clear_done(State(app): State<Arc<App>>) -> Json<Value> {
    app.manager
        .activity
        .lock()
        .await
        .retain(|row| row["state"] == "running");
    Json(json!({"ok":true}))
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
    let mut tool = None;
    if kind == "install" {
        let meta = app.catalog.read().await;
        match catalog::profiles(&meta)
            .into_iter()
            .find(|p| p.files.contains(&file))
        {
            Some(profile) => tool = Some(profile.id),
            None => {
                return failure(
                    StatusCode::BAD_REQUEST,
                    "Choose a checkpoint from the tool catalog",
                )
            }
        }
    }
    start(app, &kind, file, tool).await
}
async fn start(app: Arc<App>, kind: &str, file: String, tool: Option<String>) -> Response {
    let Ok(permit) = app.capacity.clone().try_acquire_owned() else {
        return failure(StatusCode::CONFLICT, "Wait for the current task to finish");
    };
    let kind = kind.to_owned();
    let id = uuid::Uuid::new_v4().to_string();
    let title = match kind.as_str() {
        "start" => "Start engine".to_owned(),
        "stop" => "Stop engine".to_owned(),
        "refresh" => "Refresh models".to_owned(),
        _ => {
            let meta = app.catalog.read().await;
            let name = store::array(&meta.models)
                .iter()
                .find(|m| m["file"] == file)
                .and_then(|m| m["name"].as_str())
                .unwrap_or("model")
                .to_owned();
            format!("Download {name}")
        }
    };
    let activity_kind = match kind.as_str() {
        "install" => "download",
        "refresh" => "refresh",
        _ => "engine",
    };
    app.manager
        .begin(&id, &title, activity_kind, tool.as_deref())
        .await;
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
