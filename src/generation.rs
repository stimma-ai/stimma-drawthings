use crate::generated::stimma_drawthings::_generated::config as fb;
use crate::{
    catalog::{self, Profile},
    install::{Progress, Runtime},
    media,
    proto::{self, ImageGenerationRequest},
    store, tensor,
};
use anyhow::{ensure, Context, Result};
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

fn strval(v: &Value, key: &str, default: &str) -> String {
    v[key].as_str().unwrap_or(default).to_owned()
}
fn sampler(name: &str) -> Result<String> {
    let i = catalog::SAMPLERS
        .iter()
        .position(|s| *s == name)
        .context("Unknown sampler")?;
    Ok(fb::SamplerType::ENUM_VALUES[i]
        .variant_name()
        .unwrap()
        .into())
}
pub fn configuration(profile: &Profile, p: &Value, has_image: bool) -> Result<(Vec<u8>, u32)> {
    let mut config = serde_json::to_value(fb::GenerationConfigurationT::default())?;
    let d = &profile.defaults;
    for (key, v) in d.as_object().unwrap() {
        if config.get(key).is_some() && !["sampler", "model"].contains(&key.as_str()) {
            config[key] = v.clone();
        }
    }
    config["start_width"] = json!(p["width"].as_u64().context("width missing")? / 64);
    config["start_height"] = json!(p["height"].as_u64().context("height missing")? / 64);
    config["seed"] = p["seed"].clone();
    config["steps"] = p["steps"].clone();
    config["guidance_scale"] = p.get("guidance").cloned().unwrap_or(json!(1.0));
    if let Some(v) = p.get("guidance_embed") {
        config["guidance_embed"] = v.clone();
    }
    if let Some(v) = p.get("shift") {
        config["shift"] = v.clone();
    }
    config["model"] = p["checkpoint"].clone();
    if profile.spec["refiner"] == true {
        config["refiner_model"] =
            json!(p["checkpoint"].as_str().unwrap().replace("_hne_", "_lne_"));
        config["refiner_start"] = p["refiner_start"].clone();
    }
    config["strength"] = if has_image {
        p.get("strength").cloned().unwrap_or(json!(1.0))
    } else {
        json!(1.0)
    };
    config["sampler"] = json!(sampler(p["sampler"].as_str().context("sampler missing")?)?);
    config["seed_mode"] = json!("ScaleAlike");
    config["original_image_width"] = p["width"].clone();
    config["original_image_height"] = p["height"].clone();
    config["target_image_width"] = p["width"].clone();
    config["target_image_height"] = p["height"].clone();
    config["negative_original_image_width"] = json!(512);
    config["negative_original_image_height"] = json!(512);
    if let Some(v) = p.get("mask_grow") {
        config["mask_blur_outset"] = v.clone();
    }
    if let Some(v) = p.get("mask_feather") {
        config["mask_blur"] = v.clone();
    }
    if p["upscaler"].as_str().is_some_and(|s| !s.is_empty()) {
        config["upscaler"] = p["upscaler"].clone();
        config["upscaler_scale_factor"] = p["upscaler_scale_factor"].clone();
    }
    for key in ["hires_fix_start_width", "hires_fix_start_height"] {
        if let Some(pixels) = d[key].as_u64() {
            config[key] = json!(pixels / 64);
        }
    }
    if profile.video {
        let fps = p["fps"].as_u64().context("fps missing")?;
        let duration = p["duration"].as_f64().context("duration missing")?;
        let quantum = profile.spec["frame_quantum"].as_u64().unwrap_or(8);
        let max_frames = profile.spec["max_frames"].as_u64().unwrap_or(201);
        let frames = (((duration * fps as f64) / quantum as f64).round() as u64) * quantum + 1;
        ensure!(
            frames <= max_frames,
            "Requested video exceeds {max_frames} frames"
        );
        config["fps_id"] = json!(fps);
        config["num_frames"] = json!(frames);
    }
    let loras:Vec<_>=p["loras"].as_array().into_iter().flatten().map(|l|json!({"file":l["path"],"weight":l.get("weight").cloned().unwrap_or(json!(1.0)),"mode":match l["mode"].as_str().unwrap_or("all"){"base"=>"Base","refiner"=>"Refiner",_=>"All"}})).collect();
    config["loras"] = json!(loras);
    if let Some(native) = p["native_configuration"].as_object() {
        for (k, v) in native {
            config[k] = v.clone();
        }
    }
    if let Some(controls) = p.get("_controls") {
        config["controls"] = controls.clone();
    }
    let config: fb::GenerationConfigurationT =
        serde_json::from_value(config).context("Invalid native Draw Things configuration")?;
    ensure!(
        config.start_width > 0
            && config.start_height > 0
            && config.batch_count == 1
            && config.batch_size == 1,
        "Invalid dimensions or engine batching"
    );
    ensure!(
        config.num_frames <= 201,
        "Native video frame count exceeds 201"
    );
    if config.hires_fix {
        ensure!(
            config.hires_fix_start_width > 0 && config.hires_fix_start_height > 0,
            "Hires dimensions must be positive"
        );
    }
    let seed = config.seed;
    let mut b = flatbuffers::FlatBufferBuilder::new();
    let root = config.pack(&mut b);
    b.finish(root, None);
    Ok((b.finished_data().to_vec(), seed))
}
fn content(request: &mut ImageGenerationRequest, data: Vec<u8>) -> Vec<u8> {
    let hash = Sha256::digest(&data).to_vec();
    request.contents.push(data);
    hash
}

pub async fn execute(
    runtime: &Runtime,
    tool: &str,
    parameters: &Value,
    assets: &Path,
    progress: &Progress,
    previews: bool,
) -> Result<Value> {
    let start = std::time::Instant::now();
    runtime.ensure(Some(progress)).await?;
    let catalog = store::catalog(runtime, false).await?;
    let (profile, mode, mut p) = catalog::prepare(tool, parameters, &catalog)?;
    let checkpoint = p["checkpoint"]
        .as_str()
        .context("Checkpoint missing")?
        .to_owned();
    store::ensure_model(runtime, &catalog, &checkpoint, progress).await?;
    if profile.spec["refiner"] == true {
        store::ensure_model(
            runtime,
            &catalog,
            &checkpoint.replace("_hne_", "_lne_"),
            progress,
        )
        .await?;
    }
    let width = p["width"].as_u64().unwrap() as u32;
    let height = p["height"].as_u64().unwrap() as u32;
    let input_ids: Vec<String> = p["input_images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut request = ImageGenerationRequest {
        prompt: p["prompt"].as_str().unwrap().into(),
        negative_prompt: strval(&p, "negative_prompt", ""),
        scale_factor: 1,
        chunked: true,
        user: "stimma-drawthings".into(),
        device: 2,
        shared_secret: runtime.engine.secret.clone(),
        r#override: Some(catalog.clone()),
        ..Default::default()
    };
    for (i, id) in input_ids.iter().enumerate() {
        let mut image = media::input(assets, id, width, height).await?;
        if mode == "outpaint" {
            let margins: Vec<u32> = [
                "outpaint_left",
                "outpaint_right",
                "outpaint_top",
                "outpaint_bottom",
            ]
            .iter()
            .map(|k| p[k].as_u64().unwrap_or(0) as u32)
            .collect();
            let (w, h) = (
                width + margins[0] + margins[1],
                height + margins[2] + margins[3],
            );
            ensure!(
                w <= 4096 && h <= 4096,
                "Outpaint canvas exceeds 4096 pixels"
            );
            ensure!(w > width || h > height, "Outpaint needs a nonzero margin");
            let mut canvas = image::RgbImage::new(w, h);
            image::imageops::replace(&mut canvas, &image, margins[0] as i64, margins[2] as i64);
            image = canvas;
            let mut mask = image::GrayImage::from_pixel(w, h, image::Luma([255]));
            image::imageops::replace(
                &mut mask,
                &image::GrayImage::new(width, height),
                margins[0] as i64,
                margins[2] as i64,
            );
            request.mask = Some(content(&mut request, tensor::encode_mask(&mask, false)));
            p["width"] = json!(w);
            p["height"] = json!(h);
        }
        let encoded = content(&mut request, tensor::encode_image(&image));
        if i == 0 {
            request.image = Some(encoded);
        } else {
            if !request.hints.iter().any(|h| h.hint_type == "shuffle") {
                request.hints.push(proto::HintProto {
                    hint_type: "shuffle".into(),
                    tensors: vec![],
                });
            }
            request
                .hints
                .iter_mut()
                .find(|h| h.hint_type == "shuffle")
                .unwrap()
                .tensors
                .push(proto::TensorAndWeight {
                    tensor: encoded,
                    weight: 1.0,
                });
        }
    }
    if mode == "inpaint" {
        let mask = media::mask(assets, p["mask"].as_str().unwrap(), width, height).await?;
        request.mask = Some(content(&mut request, tensor::encode_mask(&mask, false)));
    }
    let models = store::array(&catalog.models);
    let version = models
        .iter()
        .find(|m| m["file"] == checkpoint)
        .and_then(|m| m["version"].as_str())
        .unwrap_or("");
    let loras = store::array(&catalog.loras);
    let mut extra = vec![];
    for l in p["loras"].as_array().into_iter().flatten() {
        let file = l["path"].as_str().unwrap();
        let spec = loras
            .iter()
            .find(|m| m["file"] == file)
            .context("LoRA metadata is missing")?;
        ensure!(
            spec["version"].as_str() == Some(version),
            "LoRA does not match the checkpoint family"
        );
        extra.push(file.into());
        if let Some(file) = spec["alternative_decoder"].as_str() {
            extra.push(file.into());
        }
    }
    if let Some(upscaler) = p["upscaler"].as_str().filter(|s| !s.is_empty()) {
        extra.push(upscaler.into());
    }
    let controls = p["controls"].as_array().cloned().unwrap_or_default();
    let images = p["control_images"].as_array().cloned().unwrap_or_default();
    ensure!(
        controls.len() == images.len(),
        "controls and control_images must have equal lengths"
    );
    let available = store::array(&catalog.control_nets);
    let mut native_controls = vec![];
    let mut single = std::collections::HashSet::new();
    for (control, id) in controls.iter().zip(images.iter()) {
        let file = control["path"].as_str().unwrap();
        let spec = available
            .iter()
            .find(|m| m["file"] == file)
            .context("Control metadata is missing")?;
        ensure!(
            spec["version"].as_str() == Some(version),
            "Control does not match the checkpoint family"
        );
        let hint = control["hint_type"]
            .as_str()
            .or(spec["modifier"].as_str())
            .unwrap_or("custom");
        if !["pose", "shuffle"].contains(&hint) {
            ensure!(
                single.insert(hint.to_owned()),
                "Duplicate single-image control hint"
            );
        }
        let start = control["guidance_start"].as_f64().unwrap_or(0.0);
        let end = control["guidance_end"].as_f64().unwrap_or(1.0);
        ensure!(start <= end, "Control guidance start must precede end");
        let override_name = match hint {
            "custom" => "Custom",
            "depth" => "Depth",
            "canny" => "Canny",
            "scribble" => "Scribble",
            "pose" => "Pose",
            "normalbae" => "Normalbae",
            "color" => "Color",
            "lineart" => "Lineart",
            "softedge" => "Softedge",
            "seg" => "Seg",
            "inpaint" => "Inpaint",
            "ip2p" => "Ip2p",
            "shuffle" => "Shuffle",
            "mlsd" => "Mlsd",
            "tile" => "Tile",
            "blur" => "Blur",
            "lowquality" => "Lowquality",
            "gray" => "Gray",
            _ => anyhow::bail!("Unsupported control hint"),
        };
        native_controls.push(json!({"file":file,"weight":control["weight"].as_f64().unwrap_or(1.0),"guidance_start":start,"guidance_end":end,"global_average_pooling":spec["global_average_pooling"].as_bool().unwrap_or(false),"control_mode":match control["mode"].as_str().unwrap_or("balanced"){"prompt"=>"Prompt","control"=>"Control",_=>"Balanced"},"input_override":override_name}));
        let data = if hint == "scribble" {
            tensor::encode_mask(
                &media::mask(assets, id.as_str().unwrap(), width, height).await?,
                true,
            )
        } else {
            tensor::encode_image(&media::input(assets, id.as_str().unwrap(), width, height).await?)
        };
        let encoded = content(&mut request, data);
        request.hints.push(proto::HintProto {
            hint_type: hint.into(),
            tensors: vec![proto::TensorAndWeight {
                tensor: encoded,
                weight: 1.0,
            }],
        });
        extra.push(file.into());
        for key in ["image_encoder", "autoencoder"] {
            if let Some(file) = spec[key].as_str() {
                extra.push(file.into());
            }
        }
    }
    for key in ["upscaler", "face_restoration"] {
        if let Some(file) = p["native_configuration"][key]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            extra.push(file.into());
        }
    }
    for spec in store::array(&catalog.textual_inversions) {
        if let Some(keyword) = spec["keyword"].as_str().or(spec["name"].as_str()) {
            if request.prompt.contains(keyword) || request.negative_prompt.contains(keyword) {
                if let Some(file) = spec["file"].as_str() {
                    extra.push(file.into());
                }
            }
        }
    }
    if !extra.is_empty() {
        store::ensure_files(runtime, extra, progress).await?;
    }
    p["_controls"] = json!(native_controls);
    let (config, seed) = configuration(&profile, &p, !input_ids.is_empty())?;
    request.configuration = config;
    let fb = fb::root_as_generation_configuration(&request.configuration)?;
    let steps = fb.steps().max(1);
    let fps = fb.fps_id().max(1);
    if profile.video {
        runtime.encoder(Some(progress)).await?;
    }
    progress.report(0.25, "Generating").await;
    let temporary = tempfile::tempdir_in(assets)?;
    let mut client = runtime.engine.connect().await?;
    let mut stream = client.generate_image(request).await?.into_inner();
    let mut pending_image = vec![];
    let mut pending_audio = vec![];
    let mut output = vec![];
    let mut frame_count = 0usize;
    let mut audios = vec![];
    let mut last_progress = 0.25;
    while let Some(reply) = stream.message().await? {
        if let Some(signpost) = reply.current_signpost.and_then(|s| s.signpost) {
            use proto::image_generation_signpost_proto::Signpost;
            let (fraction, status) = match signpost {
                Signpost::Sampling(s) => (
                    0.25 + 0.65 * (s.step.max(0) as f64 / steps as f64).min(1.0),
                    "Sampling",
                ),
                Signpost::SecondPassSampling(s) => (
                    0.90 + 0.05 * (s.step.max(0) as f64 / steps as f64).min(1.0),
                    "Second pass",
                ),
                Signpost::ImageDecoded(_) | Signpost::SecondPassImageDecoded(_) => {
                    (0.97, "Decoding")
                }
                Signpost::ImageUpscaled(_) => (0.98, "Upscaling"),
                _ => (last_progress, "Preparing model"),
            };
            last_progress = fraction.max(last_progress);
            progress.report(last_progress, status).await;
        }
        if previews {
            if let Some(preview) = reply.preview_image {
                let preview = if image::guess_format(&preview).is_ok() {
                    preview
                } else {
                    tensor::preview(&preview, version).unwrap_or_default()
                };
                if preview.len() <= 512 * 1024 {
                    if let Ok(format) = image::guess_format(&preview) {
                        let mime = match format {
                            image::ImageFormat::Png => Some("image/png"),
                            image::ImageFormat::Jpeg => Some("image/jpeg"),
                            image::ImageFormat::WebP => Some("image/webp"),
                            _ => None,
                        };
                        if let Some(mime) = mime {
                            let _=progress.tx.send(json!({"jsonrpc":"2.0","method":"tools.progress","params":{"request_id":progress.id,"progress":last_progress,"preview":{"mime":mime,"data":base64::engine::general_purpose::STANDARD.encode(preview)}}})).await;
                        }
                    }
                }
            }
        }
        for (i, chunk) in reply.generated_images.into_iter().enumerate() {
            ensure!(
                pending_image.len() + chunk.len() <= 1024 * 1024 * 1024,
                "Image tensor exceeds 1 GiB"
            );
            pending_image.extend(chunk);
            if reply.chunk_state == 1 {
                ensure!(i == 0, "Invalid chunk stream");
                continue;
            }
            let bytes = std::mem::take(&mut pending_image);
            let pngs = tokio::task::spawn_blocking(move || tensor::pngs(&bytes)).await??;
            for png in pngs {
                if profile.video {
                    tokio::fs::write(
                        temporary.path().join(format!("frame-{frame_count:06}.png")),
                        png,
                    )
                    .await?;
                    frame_count += 1;
                    ensure!(frame_count <= 201, "Engine returned too many video frames");
                } else {
                    let id = format!("{}.png", uuid::Uuid::new_v4());
                    tokio::fs::write(assets.join(&id), png).await?;
                    output.push(json!({"asset_id":id,"type":"image","role":"primary"}));
                }
            }
        }
        for (i, chunk) in reply.generated_audio.into_iter().enumerate() {
            ensure!(
                pending_audio.len() + chunk.len() <= 128 * 1024 * 1024,
                "Audio tensor too large"
            );
            pending_audio.extend(chunk);
            if reply.chunk_state == 1 {
                ensure!(i == 0, "Invalid audio chunks");
                continue;
            }
            if p["generate_audio"] != false {
                audios.push(tensor::audio(&pending_audio)?);
            }
            pending_audio.clear();
        }
    }
    ensure!(
        pending_image.is_empty() && pending_audio.is_empty(),
        "Incomplete final tensor chunk"
    );
    if profile.video {
        let path = media::video(
            runtime,
            temporary.path(),
            frame_count,
            fps,
            &audios,
            progress,
        )
        .await?;
        let id = format!("{}.mp4", uuid::Uuid::new_v4());
        tokio::fs::rename(path, assets.join(&id)).await?;
        output.push(json!({"asset_id":id,"type":"video","role":"primary"}));
    }
    ensure!(!output.is_empty(), "Draw Things returned no output");
    progress.report(1.0, "Complete").await;
    Ok(
        json!({"request_id":progress.id,"success":true,"output":{"assets":output},"metadata":{"actual_seed":seed,"generation_time":start.elapsed().as_secs_f64()}}),
    )
}
