use crate::{
    catalog, generation,
    install::{self, Progress, Runtime},
    proto::MetadataOverride,
    store,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{mpsc, RwLock, Semaphore};

pub struct App {
    pub runtime: Runtime,
    pub manager: Arc<crate::manager::Manager>,
    pub events: tokio::sync::broadcast::Sender<Value>,
    pub catalog: RwLock<MetadataOverride>,
    pub capacity: Arc<Semaphore>,
    pub jobs: std::sync::atomic::AtomicUsize,
}
impl App {
    pub async fn refresh(&self, network: bool) -> Result<()> {
        let catalog = store::catalog(&self.runtime, network).await?;
        *self.catalog.write().await = catalog;
        Ok(())
    }
    pub fn registration(&self, websocket: bool) -> Value {
        let mut p = json!({"stp_version":"1.0","provider_id":"stimma-drawthings","provider_name":"Draw Things","server":concat!("stimma-drawthings/",env!("CARGO_PKG_VERSION")),"max_concurrent":1,"capabilities":{"cancel":true,"provider_state":true}});
        if websocket {
            p["asset_endpoint"] = json!("/assets");
            p["presentation"] = json!({"management_url":crate::manager::PREFIX,"icon":format!("data:image/png;base64,{}",base64::Engine::encode(&base64::engine::general_purpose::STANDARD,include_bytes!("../manager-ui/public/drawthings.png")))});
        }
        json!({"jsonrpc":"2.0","id":"register","method":"provider.register","params":p})
    }
}
struct Job {
    id: String,
    handle: tokio::task::JoinHandle<Result<Value>>,
}
async fn send(tx: &mpsc::Sender<Value>, value: Value) -> Result<()> {
    tx.send(value).await.context("Host disconnected")
}
async fn reply(tx: &mpsc::Sender<Value>, id: Value, value: Value) -> Result<()> {
    send(tx, json!({"jsonrpc":"2.0","id":id,"result":value})).await
}
async fn error(tx: &mpsc::Sender<Value>, id: Value, code: i32, message: String) -> Result<()> {
    send(
        tx,
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
    )
    .await
}
fn queue_value(app: &App) -> Value {
    use std::sync::atomic::Ordering;
    let total = app.jobs.load(Ordering::SeqCst);
    let running = usize::from(app.capacity.available_permits() == 0).min(total);
    json!({"jsonrpc":"2.0","method":"queue.status","params":{"running":running,"queued":total-running,"capacity":1}})
}
async fn queue(app: &App, tx: &mpsc::Sender<Value>) -> Result<()> {
    send(tx, queue_value(app)).await
}
struct JobTicket {
    app: Arc<App>,
    tx: mpsc::Sender<Value>,
}
impl Drop for JobTicket {
    fn drop(&mut self) {
        self.app
            .jobs
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        let _ = self.tx.try_send(queue_value(&self.app));
    }
}
async fn terminal(
    app: &App,
    tx: &mpsc::Sender<Value>,
    id: &str,
    outcome: Result<Value>,
) -> Result<()> {
    let value=outcome.unwrap_or_else(|e|json!({"request_id":id,"success":false,"error":{"code":"EXECUTION_FAILED","message":format!("{e:#}")}}));
    send(
        tx,
        json!({"jsonrpc":"2.0","method":"tools.result","params":value}),
    )
    .await?;
    queue(app, tx).await
}

pub async fn session(
    app: Arc<App>,
    assets: PathBuf,
    mut input: mpsc::Receiver<Value>,
    tx: mpsc::Sender<Value>,
    websocket: bool,
) -> Result<()> {
    tokio::fs::create_dir_all(&assets).await?;
    let session_prefix = format!("{}/", uuid::Uuid::new_v4());
    let observer_prefix = session_prefix.clone();
    let manager = app.manager.clone();
    let host_tx = tx;
    let (tx, mut observed_rx) = mpsc::channel::<Value>(32);
    tokio::spawn(async move {
        while let Some(message) = observed_rx.recv().await {
            manager.observe(&observer_prefix, &message).await;
            if host_tx.send(message).await.is_err() {
                break;
            }
        }
    });
    let mut events = app.events.subscribe();
    send(&tx, app.registration(websocket)).await?;
    send(&tx, json!({"jsonrpc":"2.0","method":"provider.state","params":{"state":if app.capacity.available_permits()==0 {"in_progress"} else {"ready"}}})).await?;
    let mut jobs: Vec<Job> = Vec::new();
    let mut uploads: HashMap<String, Value> = HashMap::new();
    // Hosts advertising `tool_status` receive not-yet-downloaded tools flagged
    // needs_setup (counted, never offered); others only see ready tools.
    let mut tool_status = false;
    let outcome=async {
        loop {
            let message=tokio::select! {
                event=events.recv()=>{ if let Ok(event)=event { send(&tx,event).await?; } continue; },
                message=input.recv()=>match message{Some(m)=>m,None=>break},
                outcome=async{if jobs.is_empty(){std::future::pending().await}else{let (outcome,index,_) = futures_util::future::select_all(jobs.iter_mut().map(|j| &mut j.handle)).await;(outcome,index)}}=>{
                    let finished=jobs.remove(outcome.1);terminal(&app,&tx,&finished.id,outcome.0.map_err(anyhow::Error::from).and_then(|r|r)).await?;continue;
                }
            };
            let Some(method)=message["method"].as_str() else{
                if message["id"]=="register" && message["result"]["capabilities"]["tool_status"]==true { tool_status=true; }
                if message.get("result").is_none() && message.get("error").is_none(){error(&tx,message["id"].clone(),-32600,"Invalid request".into()).await?;}continue;};let id=message["id"].clone();let params=&message["params"];
            match method {
                "__parse_error"=>error(&tx,Value::Null,-32700,"Parse error".into()).await?,
                "provider.disconnect"=>break,
                "ping"|"provider.ping"=>reply(&tx,id,json!({})).await?,
                "tools.list"=>reply(&tx,id,catalog::descriptors_for(&*app.catalog.read().await,tool_status)).await?,
                "tools.refresh"=>{
                    match app.refresh(true).await {Ok(())=>reply(&tx,id,catalog::descriptors_for(&*app.catalog.read().await,tool_status)).await?,Err(e)=>error(&tx,id,-32000,e.to_string()).await?,}
                },
                "drawthings.status"=>{
                    let live=app.runtime.engine.catalog().await;let catalog=app.catalog.read().await;
                    reply(&tx,id,json!({"ready":live.is_ok(),"managed":app.runtime.managed,"busy":app.capacity.available_permits()==0,"models":store::array(&catalog.models),"loras":store::array(&catalog.loras),"controls":store::array(&catalog.control_nets),"upscalers":store::array(&catalog.upscalers),"textual_inversions":store::array(&catalog.textual_inversions),"error":live.err().map(|e|format!("{e:#}"))})).await?;
                },
                "tools.execute"=>{
                    let request_id=params["request_id"].as_str().unwrap_or("").to_owned();let tool=params["tool_id"].as_str().unwrap_or("").to_owned();let parameters=params["parameters"].clone();
                    if request_id.is_empty(){error(&tx,id,-32602,"request_id is required".into()).await?;continue;}
                    if let Err(e)=catalog::prepare(&tool,&parameters,&*app.catalog.read().await){error(&tx,id,-32602,e.to_string()).await?;continue;}
                    if jobs.iter().any(|j| j.id == request_id){error(&tx,id,-32602,"Duplicate request_id".into()).await?;continue;}
                    let title=catalog::profiles(&*app.catalog.read().await).into_iter().find(|p| tool==p.id||tool.starts_with(&format!("{}-",p.id))).map(|p| p.name).unwrap_or_else(|| tool.clone());
                    app.manager.begin(&format!("{session_prefix}{request_id}"), &title, "generation", Some(&tool)).await;
                    app.jobs.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
                    let ticket=JobTicket{app:app.clone(),tx:tx.clone()};
                    reply(&tx,id,json!({"accepted":true})).await?;queue(&app,&tx).await?;
                    let app=app.clone();let assets=assets.clone();let progress=Progress{tx:tx.clone(),id:request_id.clone()};let previews=params["options"]["preview_frames"]!=false;
                    let handle=tokio::spawn(async move{let _ticket=ticket;let _permit=app.capacity.clone().acquire_owned().await?;queue(&app,&progress.tx).await?;generation::execute(&app.runtime,&tool,&parameters,&assets,&progress,previews).await});jobs.push(Job{id:request_id,handle});
                },
                "tools.cancel"=>{
                    let Some(index)=jobs.iter().position(|j|Some(j.id.as_str())==params["request_id"].as_str()) else {reply(&tx,id,json!({"cancelled":false})).await?;continue;};
                    let task=jobs.remove(index);task.handle.abort();let outcome=task.handle.await;
                    if let Ok(result)=outcome {reply(&tx,id,json!({"cancelled":false})).await?;terminal(&app,&tx,&task.id,result).await?;}
                    else{reply(&tx,id,json!({"cancelled":true})).await?;send(&tx,json!({"jsonrpc":"2.0","method":"tools.result","params":{"request_id":task.id,"success":false,"error":{"code":"CANCELLED","message":"Generation cancelled"}}})).await?;queue(&app,&tx).await?;}
                },
                "tools.upload"=>{
                    let accepted=prepare_upload(params,&*app.catalog.read().await,&uploads);
                    match accepted{Ok((key,entry))=>{reply(&tx,id,json!({"accepted":true,"asset_id":entry["asset_id"]})).await?;uploads.insert(key,entry);},Err(e)=>reply(&tx,id,json!({"accepted":false,"error":e.to_string()})).await?,}
                },
                "tools.upload_complete"=>{
                    let Some(pending)=uploads.get(params["upload_id"].as_str().unwrap_or("")).cloned()else{reply(&tx,id,json!({"success":false,"error":"Unknown upload_id"})).await?;continue;};
                    let Ok(_permit)=app.capacity.clone().try_acquire_owned()else{reply(&tx,id,json!({"success":false,"error":"Engine is busy; retry upload"})).await?;continue;};
                    uploads.remove(params["upload_id"].as_str().unwrap_or(""));
                    let result=install_lora(&app,&assets,&pending).await;
                    match result{Ok(file)=>{app.refresh(false).await?;reply(&tx,id,json!({"success":true,"installed_path":file})).await?;},Err(e)=>reply(&tx,id,json!({"success":false,"error":format!("{e:#}")})).await?,}
                    let _=tokio::fs::remove_file(assets.join(pending["asset_id"].as_str().unwrap())).await;
                },
                _=>if !id.is_null(){error(&tx,id,-32601,"Method not found".into()).await?;},
            }
        }Ok::<(),anyhow::Error>(())
    }.await;
    for task in jobs {
        task.handle.abort();
        let _ = task.handle.await;
        app.manager.observe(&session_prefix, &json!({"method":"tools.result","params":{"request_id":task.id,"success":false,"error":{"code":"CANCELLED","message":"Client disconnected"}}})).await;
    }
    for pending in uploads.values() {
        if let Some(id) = pending["asset_id"].as_str() {
            let _ = tokio::fs::remove_file(assets.join(id)).await;
        }
    }
    outcome
}
fn prepare_upload(
    params: &Value,
    catalog: &MetadataOverride,
    pending: &HashMap<String, Value>,
) -> Result<(String, Value)> {
    let id = params["upload_id"].as_str().context("upload_id required")?;
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Invalid upload_id"
    );
    ensure!(
        !pending.contains_key(id) && pending.len() < 8,
        "Duplicate or excessive pending uploads"
    );
    ensure!(
        params["parameter"] == "loras",
        "Only LoRA uploads are supported"
    );
    let file = params["filename"].as_str().context("filename required")?;
    install::safe_name(file)?;
    ensure!(file.ends_with(".safetensors"), "Expected .safetensors");
    let size = params["file_size"].as_u64().context("file_size required")?;
    ensure!(
        (10..=2 * 1024 * 1024 * 1024).contains(&size),
        "LoRA must be at most 2 GiB"
    );
    let tool = params["tool_id"].as_str().context("tool_id required")?;
    ensure!(
        catalog::descriptors(catalog)["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == tool && t["parameter_schema"]["properties"].get("loras").is_some()),
        "Tool does not accept LoRA uploads"
    );
    let (family, _) = tool
        .split_once("-inpaint")
        .or_else(|| tool.split_once("-outpaint"))
        .unwrap_or((tool, ""));
    ensure!(
        catalog::profiles(catalog).iter().any(|p| p.id == family),
        "Unknown tool"
    );
    Ok((
        id.into(),
        json!({"asset_id":format!("{}.safetensors",uuid::Uuid::new_v4()),"filename":file,"file_size":size,"family":family}),
    ))
}
async fn install_lora(app: &App, assets: &Path, pending: &Value) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let source = crate::media::asset(assets, pending["asset_id"].as_str().unwrap()).await?;
    ensure!(
        tokio::fs::metadata(&source).await?.len() == pending["file_size"].as_u64().unwrap(),
        "Uploaded size does not match declared size"
    );
    let mut file = tokio::fs::File::open(&source).await?;
    let mut header = [0; 8];
    file.read_exact(&mut header).await?;
    let len = u64::from_le_bytes(header);
    ensure!(
        len > 0 && len <= 16 * 1024 * 1024,
        "Invalid safetensors header length"
    );
    let mut header = vec![0; len as usize];
    file.read_exact(&mut header).await?;
    let metadata: Value = serde_json::from_slice(&header)?;
    ensure!(metadata.is_object(), "Invalid safetensors header");
    let size = pending["file_size"].as_u64().unwrap();
    let payload = size
        .checked_sub(8 + len)
        .context("Truncated safetensors header")?;
    let mut count = 0;
    for (key, tensor) in metadata.as_object().unwrap() {
        if key == "__metadata__" {
            continue;
        }
        let offsets = tensor["data_offsets"]
            .as_array()
            .context("Missing tensor offsets")?;
        ensure!(offsets.len() == 2, "Invalid tensor offsets");
        let start = offsets[0].as_u64().context("Invalid offset")?;
        let end = offsets[1].as_u64().context("Invalid offset")?;
        ensure!(
            start <= end && end <= payload,
            "Tensor offsets exceed uploaded data"
        );
        count += 1;
    }
    ensure!(count > 0, "No tensors found");
    app.runtime.ensure(None).await?;
    let converter = app.runtime.converter(None).await?;
    let dir = tempfile::tempdir_in(&app.runtime.state)?;
    let hash = install::digest(&source).await?;
    let name = format!("stp_lora_{}", &hash[..16]);
    let child = tokio::process::Command::new(converter)
        .args(["--file"])
        .arg(&source)
        .args(["--name", &name, "--output-directory"])
        .arg(dir.path())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(std::time::Duration::from_secs(1800), child)
        .await
        .context("LoRA conversion timed out")??;
    ensure!(output.status.success(), "LoRA converter failed");
    let mut spec: Value =
        serde_json::from_slice(&output.stdout).context("Invalid converter metadata")?;
    let filename = spec["file"]
        .as_str()
        .context("Converter returned no file")?
        .to_owned();
    install::safe_name(&filename)?;
    let catalog = store::catalog(&app.runtime, false).await?;
    let profile = catalog::profiles(&catalog)
        .into_iter()
        .find(|p| p.id == pending["family"])
        .context("Upload family unavailable")?;
    let models = store::array(&catalog.models);
    ensure!(
        models.iter().any(|m| profile
            .files
            .contains(&m["file"].as_str().unwrap_or("").to_owned())
            && m["version"] == spec["version"]),
        "LoRA targets a different model family"
    );
    let path = tokio::fs::canonicalize(dir.path().join(&filename)).await?;
    ensure!(
        path.starts_with(dir.path()),
        "Converter output must stay in its directory"
    );
    app.runtime.engine.upload(&filename, &path, None).await?;
    spec["name"] = pending["filename"].clone();
    spec["prefix"] = json!("");
    store::add_lora(&app.runtime, spec).await?;
    Ok(filename)
}
