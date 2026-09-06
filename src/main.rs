use anyhow::{Context, Result};
use clap::Parser;
use std::{path::PathBuf, sync::Arc};
use stimma_drawthings::{
    engine::Engine,
    install::{self, Runtime},
    provider::App,
    store, transport,
};
use tokio::sync::{Mutex, RwLock, Semaphore};

#[derive(Parser)]
#[command(
    version,
    about = "Draw Things as a standalone STP provider. Downloads the local engine on first generation."
)]
struct Args {
    #[arg(long, conflicts_with = "websocket")]
    stdio: bool,
    #[arg(long)]
    websocket: bool,
    #[arg(long, default_value = "127.0.0.1:8765")]
    bind: std::net::SocketAddr,
    /// Attach to an existing engine; otherwise manage a local engine automatically.
    #[arg(long, conflicts_with = "engine")]
    endpoint: Option<String>,
    /// Use an installed gRPCServerCLI instead of downloading the pinned engine.
    #[arg(long)]
    engine: Option<PathBuf>,
    #[arg(long)]
    models_dir: Option<PathBuf>,
    #[arg(long)]
    state_path: Option<PathBuf>,
    #[arg(long)]
    asset_path: Option<PathBuf>,
    /// Disable network catalog and component/model downloads (engine connections remain enabled).
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    lora_converter: Option<PathBuf>,
    #[arg(long)]
    ffmpeg: Option<PathBuf>,
    /// Emit the complete current tool schemas as JSON, then exit.
    #[arg(long)]
    schemas: bool,
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let state = args.state_path.unwrap_or_else(install::state_dir);
    tokio::fs::create_dir_all(&state).await?;
    let managed = args.endpoint.is_none();
    let endpoint = if let Some(endpoint) = args.endpoint {
        endpoint
    } else {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        format!("http://127.0.0.1:{}", listener.local_addr()?.port())
    };
    let runtime = Runtime {
        engine: Engine {
            endpoint,
            secret: std::env::var("DRAWTHINGS_SHARED_SECRET").ok(),
        },
        state: state.clone(),
        models: args.models_dir,
        binary: args.engine,
        managed,
        offline: args.offline,
        converter: args.lora_converter,
        ffmpeg: args.ffmpeg,
        child: Mutex::new(None),
    };
    let catalog = store::catalog(&runtime, false).await?;
    let app = Arc::new(App {
        runtime,
        manager: Arc::new(stimma_drawthings::manager::Manager::default()),
        events: tokio::sync::broadcast::channel(64).0,
        catalog: RwLock::new(catalog),
        capacity: Arc::new(Semaphore::new(1)),
        jobs: std::sync::atomic::AtomicUsize::new(0),
    });
    if args.schemas {
        println!(
            "{}",
            serde_json::to_string_pretty(&stimma_drawthings::catalog::descriptors(
                &*app.catalog.read().await
            ))?
        );
        return Ok(());
    }
    let assets = args
        .asset_path
        .or_else(|| std::env::var_os("ASSET_PATH").map(PathBuf::from))
        .unwrap_or_else(|| state.join("assets"));
    let result = tokio::select! {
        result=async{if args.websocket{transport::serve(app.clone(),assets,args.bind,std::env::var("STIMMA_DRAWTHINGS_TOKEN").ok()).await}else{transport::stdio(app.clone(),assets).await}}=>result,
        result=tokio::signal::ctrl_c()=>result.context("Signal handler failed"),
    };
    app.runtime.stop().await?;
    result
}
