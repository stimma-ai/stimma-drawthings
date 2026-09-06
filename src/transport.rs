use crate::{
    install,
    provider::{self, App},
};
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{ws::Message, DefaultBodyLimit, Path, State, WebSocketUpgrade},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
};

pub async fn stdio(app: Arc<App>, assets: PathBuf) -> Result<()> {
    let (in_tx, in_rx) = mpsc::channel(32);
    let (out_tx, mut out_rx) = mpsc::channel::<Value>(32);
    let reader = tokio::spawn(async move {
        let mut reader = BufReader::new(tokio::io::stdin());
        loop {
            let mut line = Vec::new();
            match (&mut reader)
                .take(16 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
                .await
            {
                Ok(0) | Err(_) => break,
                _ if line.len() > 16 * 1024 * 1024 => break,
                _ => {}
            }
            let value = serde_json::from_slice(&line)
                .unwrap_or_else(|_| serde_json::json!({"method":"__parse_error"}));
            if in_tx.send(value).await.is_err() {
                break;
            }
        }
    });
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(message) = out_rx.recv().await {
            let mut bytes = serde_json::to_vec(&message)?;
            bytes.push(b'\n');
            out.write_all(&bytes).await?;
            out.flush().await?;
        }
        Ok::<(), anyhow::Error>(())
    });
    let result = provider::session(app, assets, in_rx, out_tx, false).await;
    reader.abort();
    let _ = reader.await;
    writer.await??;
    result
}
#[derive(Clone)]
struct Web {
    app: Arc<App>,
    assets: PathBuf,
    token: Option<String>,
}
async fn auth(State(state): State<Web>, request: Request<Body>, next: Next) -> Response {
    if let Some(token) = &state.token {
        let expected = format!("Bearer {token}");
        if request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            != Some(expected.as_str())
        {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    next.run(request).await
}
async fn websocket(State(state): State<Web>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(16 * 1024 * 1024)
        .on_upgrade(move |socket| async move {
            let (mut sink, mut stream) = socket.split();
            let (in_tx, in_rx) = mpsc::channel(32);
            let (out_tx, mut out_rx) = mpsc::channel::<Value>(32);
            let reader = tokio::spawn(async move {
                while let Some(Ok(message)) = stream.next().await {
                    match message {
                        Message::Text(text) => {
                            let value = serde_json::from_str(&text)
                                .unwrap_or_else(|_| serde_json::json!({"method":"__parse_error"}));
                            if in_tx.send(value).await.is_err() {
                                break;
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            });
            let writer = tokio::spawn(async move {
                while let Some(message) = out_rx.recv().await {
                    if sink
                        .send(Message::Text(message.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
            let _ = provider::session(state.app, state.assets, in_rx, out_tx, true).await;
            reader.abort();
            writer.abort();
        })
}
async fn get_asset(State(state): State<Web>, Path(id): Path<String>) -> Response {
    match crate::media::asset(&state.assets, &id).await {
        Ok(path) => match tokio::fs::File::open(path).await {
            Ok(file) => {
                let stream = async_stream::stream! {use tokio::io::AsyncReadExt;let mut file=file;let mut buf=vec![0;1024*1024];loop{match file.read(&mut buf).await{Ok(0)=>break,Ok(n)=>yield Ok::<_,std::io::Error>(buf[..n].to_vec()),Err(e)=>{yield Err(e);break;}}}};
                let mime = if id.ends_with(".png") {
                    "image/png"
                } else if id.ends_with(".mp4") {
                    "video/mp4"
                } else {
                    "application/octet-stream"
                };
                ([("content-type", mime)], Body::from_stream(stream)).into_response()
            }
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        },
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn put_asset(State(state): State<Web>, Path(id): Path<String>, body: Body) -> Response {
    let result = async {
        install::safe_name(&id)?;
        let temp = tempfile::NamedTempFile::new_in(&state.assets)?;
        let mut file = tokio::fs::File::from_std(temp.reopen()?);
        let mut stream = body.into_data_stream();
        let mut size = 0u64;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            size += bytes.len() as u64;
            anyhow::ensure!(size <= 2 * 1024 * 1024 * 1024, "Asset exceeds 2 GiB");
            file.write_all(&bytes).await?;
        }
        file.sync_all().await?;
        drop(file);
        temp.persist(state.assets.join(id)).map_err(|e| e.error)?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    match result {
        Ok(()) => StatusCode::OK.into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, "Asset upload failed").into_response(),
    }
}
async fn delete_asset(State(state): State<Web>, Path(id): Path<String>) -> Response {
    if install::safe_name(&id).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match tokio::fs::remove_file(state.assets.join(id)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
pub async fn serve(
    app: Arc<App>,
    assets: PathBuf,
    bind: std::net::SocketAddr,
    token: Option<String>,
) -> Result<()> {
    anyhow::ensure!(
        bind.ip().is_loopback() || token.as_ref().is_some_and(|t| !t.is_empty()),
        "Non-loopback WebSocket servers require STIMMA_DRAWTHINGS_TOKEN"
    );
    tokio::fs::create_dir_all(&assets).await?;
    let state = Web { app, assets, token };
    let router = Router::new()
        .route("/stp-v1", get(websocket))
        .route(
            "/assets/{id}",
            get(get_asset).put(put_asset).delete(delete_asset),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .context("Binding STP WebSocket server")?;
    eprintln!("Draw Things STP listening on {bind}");
    axum::serve(listener, router).await?;
    Ok(())
}
