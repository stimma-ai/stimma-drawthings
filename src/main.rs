use anyhow::{ensure, Context, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use stimma_drawthings::{
    engine::{self, Engine},
    proto, tensor,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

#[derive(Parser)]
#[command(
    version,
    about = "Standalone Draw Things STP provider (Rust feasibility slice)"
)]
struct Args {
    #[arg(long)]
    stdio: bool,
    #[arg(long, default_value = "http://127.0.0.1:7859")]
    endpoint: String,
    /// Launch this independently installed gRPCServerCLI; never owns an attached server.
    #[arg(long)]
    engine: Option<PathBuf>,
    /// Defaults to the Draw Things desktop application's model store on macOS.
    #[arg(long)]
    models_dir: Option<PathBuf>,
    #[arg(long)]
    asset_path: Option<PathBuf>,
}

type Output = Arc<Mutex<tokio::io::Stdout>>;
async fn send(out: &Output, value: Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(&value)?;
    bytes.push(b'\n');
    let mut stdout = out.lock().await;
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}
async fn result(out: &Output, id: Value, value: Value) -> Result<()> {
    send(out, json!({"jsonrpc":"2.0","id":id,"result":value})).await
}
async fn error(out: &Output, id: Value, message: String) -> Result<()> {
    send(
        out,
        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":message}}),
    )
    .await
}
async fn queue(out: &Output, running: u32) -> Result<()> {
    send(out,json!({"jsonrpc":"2.0","method":"queue.status","params":{"queued":0,"running":running,"capacity":1}})).await
}

async fn generate(
    engine: Engine,
    mut request: proto::ImageGenerationRequest,
    out: Output,
    id: String,
    assets: PathBuf,
    seed: u64,
    steps: u64,
) -> Result<Value> {
    let start = Instant::now();
    request.shared_secret = engine.secret.clone();
    let mut client = engine.connect().await?;
    let mut stream = client.generate_image(request).await?.into_inner();
    let mut pending = Vec::new();
    let mut output = Vec::new();
    while let Some(response) = stream.message().await? {
        if let Some(proto::image_generation_signpost_proto::Signpost::Sampling(sampling)) =
            response.current_signpost.and_then(|s| s.signpost)
        {
            send(&out,json!({"jsonrpc":"2.0","method":"tools.progress","params":{"request_id":id,"progress":(sampling.step.max(0) as f64 / steps.max(1) as f64).min(1.0)}})).await?;
        }
        ensure!(
            response.generated_audio.is_empty(),
            "Audio output is not implemented in this feasibility slice"
        );
        for (index, bytes) in response.generated_images.into_iter().enumerate() {
            ensure!(
                pending
                    .len()
                    .checked_add(bytes.len())
                    .is_some_and(|n| n <= 256 * 1024 * 1024),
                "Image tensor exceeds 256 MiB limit"
            );
            if response.chunk_state == proto::ChunkState::MoreChunks as i32 {
                ensure!(index == 0, "Unexpected parallel tensor chunks");
                pending.extend(bytes);
            } else {
                pending.extend(bytes);
                for png in tensor::pngs(&pending)? {
                    let asset = format!("{}.png", uuid::Uuid::new_v4());
                    tokio::fs::write(assets.join(&asset), png).await?;
                    output.push(json!({"asset_id":asset,"type":"image","role":"primary"}));
                }
                pending.clear();
            }
        }
    }
    ensure!(
        pending.is_empty(),
        "Draw Things ended before final tensor chunk"
    );
    ensure!(!output.is_empty(), "Draw Things returned no image");
    Ok(
        json!({"request_id":id,"success":true,"output":{"assets":output},"metadata":{"actual_seed":seed,"generation_time":start.elapsed().as_secs_f64()}}),
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let engine = Engine {
        endpoint: args.endpoint.clone(),
        secret: std::env::var("DRAWTHINGS_SHARED_SECRET").ok(),
    };
    let assets = args
        .asset_path
        .or_else(|| std::env::var_os("ASSET_PATH").map(PathBuf::from));
    let out = Arc::new(Mutex::new(tokio::io::stdout()));
    // Register independently of engine health so discovery/setup errors remain actionable.
    send(&out,json!({"jsonrpc":"2.0","id":"register","method":"provider.register","params":{
        "stp_version":"1.0","provider_id":"stimma-drawthings","provider_name":"Draw Things",
        "server":concat!("stimma-drawthings/",env!("CARGO_PKG_VERSION")),"max_concurrent":1,"capabilities":{"cancel":true}
    }})).await?;

    let mut child = None;
    if let Some(binary) = args.engine {
        let models = args
            .models_dir
            .or_else(|| {
                if cfg!(target_os = "macos") {
                    std::env::var_os("HOME").map(|h| {
                        PathBuf::from(h)
                            .join("Library/Containers/com.liuliu.draw-things/Data/Documents/Models")
                    })
                } else {
                    None
                }
            })
            .context("--models-dir is required on this platform")?;
        ensure!(models.is_dir(), "Model directory does not exist");
        let port = args
            .endpoint
            .strip_prefix("http://127.0.0.1:")
            .context("Managed mode requires an http://127.0.0.1:PORT endpoint")?
            .parse::<u16>()?;
        ensure!(port != 0, "Port must not be zero");
        ensure!(
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err(),
            "Managed engine port is already in use; use attach mode or another port"
        );
        child = Some(
            tokio::process::Command::new(binary)
                .arg(models)
                .args([
                    "--address",
                    "127.0.0.1",
                    "--port",
                    &port.to_string(),
                    "--name",
                    "Stimma Draw Things",
                    "--no-tls",
                    "--no-response-compression",
                    "--model-browser",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .context("Could not start engine")?,
        );
        let mut engine_logs = child.as_mut().unwrap().stdout.take().unwrap();
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut engine_logs, &mut tokio::io::stderr()).await;
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if engine.catalog().await.is_ok() {
                break;
            }
            if child.as_mut().unwrap().try_wait()?.is_some() {
                anyhow::bail!("Managed engine exited during startup");
            }
            ensure!(
                Instant::now() < deadline,
                "Managed engine readiness timed out"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut job: Option<(String, tokio::task::JoinHandle<()>)> = None;
    while let Some(line) = lines.next_line().await? {
        let message: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                continue;
            }
        };
        let Some(method) = message["method"].as_str() else {
            continue;
        };
        let id = message["id"].clone();
        let params = &message["params"];
        if job.as_ref().is_some_and(|(_, h)| h.is_finished()) {
            job = None;
        }
        match method {
            "provider.disconnect"=>break,
            "ping"|"provider.ping"=>result(&out,id,json!({})).await?,
            "tools.list"|"tools.refresh"=>match engine.catalog().await.and_then(|c|engine::descriptors(&c)) {
                Ok(tools)=>result(&out,id,tools).await?,
                Err(e)=>error(&out,id,format!("Draw Things unavailable: {e:#}")).await?,
            },
            "drawthings.status"=>{
                let status=match engine.catalog().await {
                    Ok(c)=>json!({"ready":true,"models":engine::models(&c)?,"loras":serde_json::from_slice::<Value>(&c.loras).unwrap_or(json!([])),"controls":serde_json::from_slice::<Value>(&c.control_nets).unwrap_or(json!([])),"upscalers":serde_json::from_slice::<Value>(&c.upscalers).unwrap_or(json!([]))}),
                    Err(e)=>json!({"ready":false,"error":format!("{e:#}")}),
                }; result(&out,id,status).await?;
            },
            "tools.cancel"=>{
                let matches=job.as_ref().is_some_and(|(r,_)|Some(r.as_str())==params["request_id"].as_str());
                if matches {
                    let (request_id,handle)=job.take().unwrap();handle.abort();let _=handle.await;
                    result(&out,id,json!({"cancelled":true})).await?;
                    send(&out,json!({"jsonrpc":"2.0","method":"tools.result","params":{"request_id":request_id,"success":false,"error":{"code":"CANCELLED","message":"Generation cancelled"}}})).await?;
                    queue(&out,0).await?;
                } else {result(&out,id,json!({"cancelled":false})).await?;}
            },
            "tools.execute"=>{
                if job.is_some() {result(&out,id,json!({"accepted":false,"error":"Provider is busy"})).await?;continue;}
                let request_id=params["request_id"].as_str().unwrap_or("").to_owned();
                if request_id.is_empty() {error(&out,id,"request_id is required".into()).await?;continue;}
                let Some(assets)=assets.clone() else {error(&out,id,"STP host must supply ASSET_PATH or --asset-path".into()).await?;continue;};
                let request=match engine.catalog().await.and_then(|c|engine::generation(params["tool_id"].as_str().unwrap_or(""),&params["parameters"],&c)) {
                    Ok(r)=>r,Err(e)=>{error(&out,id,format!("{e:#}")).await?;continue;}
                };
                tokio::fs::create_dir_all(&assets).await?;
                result(&out,id,json!({"accepted":true})).await?;
                queue(&out,1).await?;
                let config=stimma_drawthings::generated::stimma_drawthings::_generated::config::root_as_generation_configuration(&request.configuration)?;
                let (seed,steps)=(config.seed() as u64,config.steps() as u64);
                let task_engine=engine.clone();let task_out=out.clone();let task_id=request_id.clone();
                let handle=tokio::spawn(async move {
                    let result=generate(task_engine,request,task_out.clone(),task_id.clone(),assets,seed,steps).await;
                    let value=result.unwrap_or_else(|e|json!({"request_id":task_id,"success":false,"error":{"code":"EXECUTION_FAILED","message":format!("{e:#}")}}));
                    let _=send(&task_out,json!({"jsonrpc":"2.0","method":"tools.result","params":value})).await;
                    let _=queue(&task_out,0).await;
                });
                job=Some((request_id,handle));
            },
            _=>send(&out,json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})).await?,
        }
    }
    if let Some((_, h)) = job {
        h.abort();
        let _ = h.await;
    }
    if let Some(mut child) = child {
        child.kill().await?;
        child.wait().await?;
    }
    Ok(())
}
