use crate::{
    install::{self, Progress, Runtime},
    proto::MetadataOverride,
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub fn array(bytes: &[u8]) -> Vec<Value> {
    serde_json::from_slice(bytes).unwrap_or_default()
}
fn merge(a: Vec<Value>, b: Vec<Value>) -> Vec<Value> {
    let mut map = BTreeMap::new();
    for item in a.into_iter().chain(b) {
        if let Some(file) = item["file"].as_str() {
            if install::safe_name(file).is_ok() {
                map.insert(file.to_owned(), item);
            }
        }
    }
    map.into_values().collect()
}
fn curated() -> Value {
    serde_json::from_str(include_str!("../data/curated_models.json")).expect("embedded catalog")
}

pub async fn cached_json(
    state: &Path,
    name: &str,
    url: &str,
    refresh: bool,
    offline: bool,
) -> Result<Value> {
    let path = state.join("catalog").join(name);
    if refresh && !offline {
        if let Ok(response) = install::http()?
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
        {
            if let Ok(response) = response.error_for_status() {
                if let Ok(bytes) = response.bytes().await {
                    if bytes.len() < 16 * 1024 * 1024 {
                        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                            write_json(&path, &value).await?;
                            return Ok(value);
                        }
                    }
                }
            }
        }
    }
    Ok(tokio::fs::read(path)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null))
}
pub async fn write_json(path: &Path, value: &Value) -> Result<()> {
    let parent = path.parent().context("State path needs parent")?;
    tokio::fs::create_dir_all(parent).await?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    tokio::fs::write(temp.path(), serde_json::to_vec_pretty(value)?).await?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub async fn catalog(runtime: &Runtime, refresh: bool) -> Result<MetadataOverride> {
    let base = curated();
    let addons: Value = serde_json::from_str(include_str!("../data/addons.json"))?;
    let online = cached_json(
        &runtime.state,
        "models.json",
        "https://models.drawthings.ai/models.json",
        refresh,
        runtime.offline,
    )
    .await?;
    let downloaded = merge(
        base["models"].as_array().cloned().unwrap_or_default(),
        online.as_array().cloned().unwrap_or_default(),
    );
    let mut live = runtime.engine.catalog().await.unwrap_or_default();
    let mut models = merge(downloaded, array(&live.models));
    let files: Vec<String> = models
        .iter()
        .filter_map(|m| m["file"].as_str().map(str::to_owned))
        .collect();
    let installed = if runtime.managed {
        let dir = runtime
            .models
            .clone()
            .or_else(|| install::desktop_models().ok());
        let mut installed = std::collections::HashMap::new();
        if let Some(dir) = dir {
            for file in files {
                installed.insert(
                    file.clone(),
                    tokio::fs::try_exists(dir.join(&file))
                        .await
                        .unwrap_or(false),
                );
            }
        }
        installed
    } else {
        runtime.engine.files_exist(files).await.unwrap_or_default()
    };
    for model in &mut models {
        model["stp_installed"] =
            json!(installed.get(model["file"].as_str().unwrap_or("")) == Some(&true));
    }
    live.models = serde_json::to_vec(&models)?;
    live.control_nets = serde_json::to_vec(&merge(
        addons["controls"].as_array().cloned().unwrap_or_default(),
        array(&live.control_nets),
    ))?;
    live.upscalers = serde_json::to_vec(&merge(
        addons["upscalers"].as_array().cloned().unwrap_or_default(),
        array(&live.upscalers),
    ))?;
    live.textual_inversions = serde_json::to_vec(&merge(
        addons["textual_inversions"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        array(&live.textual_inversions),
    ))?;
    live.loras = serde_json::to_vec(&merge(
        addons["loras"].as_array().cloned().unwrap_or_default(),
        array(&live.loras),
    ))?;
    let registry = registry_path(runtime);
    let custom = tokio::fs::read(registry)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
        .unwrap_or_default();
    live.loras = serde_json::to_vec(&merge(array(&live.loras), custom))?;
    Ok(live)
}
pub fn registry_path(runtime: &Runtime) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let identity = if runtime.managed {
        runtime
            .models
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "managed-local-model-store".into())
    } else {
        runtime.engine.endpoint.clone()
    };
    runtime.state.join(format!(
        "loras-{}.json",
        &hex::encode(Sha256::digest(identity.as_bytes()))[..16]
    ))
}
pub async fn add_lora(runtime: &Runtime, item: Value) -> Result<()> {
    let path = registry_path(runtime);
    let _lock = install::lock(&path.with_extension("lock")).await?;
    let old = tokio::fs::read(&path)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    write_json(&path, &json!(merge(old, vec![item]))).await
}
pub fn dependencies(model: &Value) -> Result<Vec<String>> {
    let mut files = BTreeSet::new();
    for key in [
        "file",
        "text_encoder",
        "autoencoder",
        "image_encoder",
        "clip_encoder",
        "t5_encoder",
        "diffusion_mapping",
    ] {
        if let Some(file) = model[key].as_str() {
            files.insert(file.to_owned());
        }
    }
    if model["text_encoder"].is_null() {
        files.insert(
            if model["version"] == "v1" {
                "clip_vit_l14_f16.ckpt"
            } else {
                "open_clip_vit_h14_f16.ckpt"
            }
            .into(),
        );
    }
    if model["autoencoder"].is_null() {
        files.insert("vae_ft_mse_840000_f16.ckpt".into());
    }
    for key in ["additional_clip_encoders", "stage_models"] {
        for v in model[key].as_array().into_iter().flatten() {
            if let Some(file) = v.as_str() {
                files.insert(file.into());
            }
        }
    }
    for v in model["latents_upscalers"].as_array().into_iter().flatten() {
        if let Some(file) = v["file"].as_str() {
            files.insert(file.into());
        }
    }
    for name in &files {
        install::safe_name(name)?;
    }
    Ok(files.into_iter().collect())
}
pub async fn ensure_files(runtime: &Runtime, files: Vec<String>, p: &Progress) -> Result<()> {
    let exists = runtime.engine.files_exist(files.clone()).await?;
    let missing: Vec<_> = files
        .into_iter()
        .filter(|f| exists.get(f) != Some(&true))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut hashes = curated()["sha256"].as_object().cloned().unwrap_or_default();
    let addons: Value = serde_json::from_str(include_str!("../data/addons.json"))?;
    hashes.extend(addons["sha256"].as_object().cloned().unwrap_or_default());
    for name in ["models_sha256.json", "uncurated_models_sha256.json"] {
        let value = cached_json(
            &runtime.state,
            name,
            &format!("https://models.drawthings.ai/{name}"),
            true,
            runtime.offline,
        )
        .await?;
        if let Some(map) = value.as_object() {
            hashes.extend(map.clone());
        }
    }
    for file in missing {
        install::safe_name(&file)?;
        let sha = hashes.get(&file).and_then(Value::as_str).context(format!(
            "No published checksum for missing file {file}; install it in Draw Things first"
        ))?;
        // Unique staging folder is removed on cancellation; no second persistent weight store.
        tokio::fs::create_dir_all(runtime.state.join("downloads")).await?;
        let temp = tempfile::tempdir_in(runtime.state.join("downloads"))?;
        let local = temp.path().join(&file);
        let mut url = reqwest::Url::parse("https://static.libnnc.org/")?;
        url.path_segments_mut().unwrap().push(&file);
        install::fetch(url.as_str(), sha, &local, Some(p), runtime.offline).await?;
        runtime.engine.upload(&file, &local, Some(p)).await?;
    }
    Ok(())
}
pub async fn ensure_model(
    runtime: &Runtime,
    catalog: &MetadataOverride,
    name: &str,
    p: &Progress,
) -> Result<()> {
    let models = array(&catalog.models);
    let model = models
        .iter()
        .find(|m| m["file"] == name)
        .context("Model is absent from catalog")?;
    ensure_files(runtime, dependencies(model)?, p).await
}
