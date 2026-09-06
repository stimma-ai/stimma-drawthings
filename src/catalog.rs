use crate::{proto::MetadataOverride, store};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

pub const SAMPLERS: [&str; 20] = [
    "DPM++ 2M Karras",
    "Euler A",
    "DDIM",
    "PLMS",
    "DPM++ SDE Karras",
    "UniPC",
    "LCM",
    "Euler A Substep",
    "DPM++ SDE Substep",
    "TCD",
    "Euler A Trailing",
    "DPM++ SDE Trailing",
    "DPM++ 2M AYS",
    "Euler A AYS",
    "DPM++ SDE AYS",
    "DPM++ 2M Trailing",
    "DDIM Trailing",
    "UniPC Trailing",
    "UniPC AYS",
    "TCD Trailing",
];
#[derive(Clone, Debug)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub video: bool,
    pub image: bool,
    pub inpaint: bool,
    pub defaults: Value,
    pub files: Vec<String>,
    pub spec: Value,
    pub version: String,
}
fn family(m: &Value) -> String {
    let file = m["file"].as_str().unwrap_or("");
    let version = m["version"].as_str().unwrap_or("");
    match version {
        "z_image" if file.contains("turbo") => "z-image-turbo".into(),
        "flux2_9b" if file.contains("klein") && !file.contains("base") => "flux2-klein-9b".into(),
        "krea_2" if file.contains("turbo") => "krea-2".into(),
        "ideogram_4" => if file.contains("instant") {
            "ideogram-4-instant"
        } else if file.contains("fast") {
            "ideogram-4-fast"
        } else {
            "ideogram-4"
        }
        .into(),
        "sdxl_base_v0.9" => "sdxl".into(),
        "ltx2.3" => if file.contains("distill") {
            "ltx-2.3-distilled"
        } else {
            "ltx-2.3"
        }
        .into(),
        _ => String::new(),
    }
}
pub fn profiles(catalog: &MetadataOverride) -> Vec<Profile> {
    let specs: Value =
        serde_json::from_str(include_str!("../data/profiles.json")).expect("embedded profiles");
    let installed: std::collections::HashSet<String> = store::array(&catalog.models)
        .iter()
        .filter(|m| m["stp_installed"] == true)
        .filter_map(|m| m["file"].as_str().map(str::to_owned))
        .collect();
    let models = store::array(&catalog.models);
    specs
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(id, spec)| {
            let mut files: Vec<String> = models
                .iter()
                .filter(|model| {
                    let file = model["file"].as_str().unwrap_or("");
                    if file == "flux_2_klein_9b_i8x.ckpt" || file.contains("refiner") {
                        return false;
                    }
                    if let Some(files) = spec["files"].as_array() {
                        return model["version"] == spec["version"]
                            && files.iter().any(|f| f == file);
                    }
                    if let Some(prefix) = spec["prefix"].as_str() {
                        model["version"] == spec["version"]
                            && file.starts_with(prefix)
                            && (id != "flux1-dev" || !file.contains("de_distill"))
                    } else {
                        family(model) == spec["source_profile"].as_str().unwrap_or(id)
                    }
                })
                .filter_map(|m| m["file"].as_str().map(str::to_owned))
                .collect();
            files.sort_by_key(|f| {
                (
                    !installed.contains(f),
                    !f.contains("q8p"),
                    f.contains("i8"),
                    f.contains("kv"),
                    f.len(),
                    f.clone(),
                )
            });
            files.dedup();
            let first = files.first()?;
            let version = models.iter().find(|m| m["file"] == *first)?["version"]
                .as_str()?
                .to_owned();
            Some(Profile {
                id: id.clone(),
                name: spec["display_name"].as_str().unwrap().into(),
                video: spec["kind"] == "video",
                image: spec["supports_image"] == true,
                inpaint: spec["supports_inpaint"] == true,
                defaults: spec["defaults"].clone(),
                files,
                spec: spec.clone(),
                version,
            })
        })
        .collect()
}

fn number(default: Value, min: f64, max: f64, integer: bool) -> Value {
    json!({"type":if integer{"integer"}else{"number"},"default":default,"minimum":min,"maximum":max,"x-control":"slider"})
}
fn image_array(max: usize, required: bool) -> Value {
    json!({"type":"array","default":[],"items":{"type":"string"},"maxItems":max,"minItems":if required{1}else{0},"x-control":"image_picker","x-max-items":max,"x-min-items":if required{1}else{0},"x-accept-media":{"mime_types":["image/png","image/jpeg","image/webp"],"transcode_to":"image/png"}})
}
fn names(bytes: &[u8]) -> Vec<String> {
    store::array(bytes)
        .iter()
        .filter_map(|m| m["file"].as_str().map(str::to_owned))
        .collect()
}
fn compatible(bytes: &[u8], version: &str) -> Vec<String> {
    store::array(bytes)
        .iter()
        .filter(|m| m["version"] == version)
        .filter_map(|m| m["file"].as_str().map(str::to_owned))
        .collect()
}
fn descriptor(profile: &Profile, mode: &str, catalog: &MetadataOverride) -> Value {
    let d = &profile.defaults;
    let mut props = serde_json::Map::new();
    props.insert("prompt".into(),json!({"type":"string","x-control":"prompt_editor","description":"Describe the desired output."}));
    props.insert("negative_prompt".into(),json!({"type":"string","default":d["negative_prompt"].as_str().unwrap_or(""),"x-control":"textarea"}));
    props.insert("checkpoint".into(),json!({"type":"string","enum":profile.files,"default":profile.files[0],"x-control":"dropdown"}));
    for key in ["width", "height"] {
        let mut schema = number(d[key].clone(), 64.0, 4096.0, true);
        schema["multipleOf"] = json!(64);
        schema["x-control"] = json!("resolution");
        schema["x-resolution-role"] = json!(key);
        props.insert(key.into(), schema);
    }
    props.insert("seed".into(),json!({"type":"integer","minimum":0,"maximum":4294967295u64,"default":0,"x-control":"seed"}));
    props.insert("steps".into(), number(d["steps"].clone(), 1.0, 200.0, true));
    props.insert(
        "guidance".into(),
        number(d["guidance"].clone(), 0.0, 30.0, false),
    );
    props.insert(
        "sampler".into(),
        json!({"type":"string","enum":if profile.id == "sdxl" { SAMPLERS.to_vec() } else { vec!["Euler A Trailing", "DPM++ 2M Trailing", "DDIM Trailing", "UniPC Trailing", "UniPC AYS", "TCD Trailing"] },"default":d["sampler"],"x-control":"dropdown"}),
    );
    if profile.id == "ideogram-4-fast" {
        props.get_mut("prompt").unwrap()["description"] = json!("Describe the output using Ideogram Fast's structured JSON caption format for best results.");
    }
    if profile.spec["guidance_mode"] == "embedded" {
        let mut guidance = props.remove("guidance").unwrap();
        guidance["description"] =
            json!("Distilled embedded guidance; CFG is fixed at 1 for this profile.");
        props.insert("guidance_embed".into(), guidance);
        props.remove("negative_prompt");
    }
    if profile.id != "sdxl" {
        props.insert("shift".into(), number(d["shift"].clone(), 0.1, 20.0, false));
    }
    let mut required = vec!["prompt"];
    let needs_image = profile.spec["requires_image"] == true || mode != "generate";
    if needs_image {
        required.push("input_images");
    }
    if profile.image || mode != "generate" {
        props.insert(
            "input_images".into(),
            image_array(
                if mode == "generate" {
                    profile.spec["max_images"].as_u64().unwrap_or(1) as usize
                } else {
                    1
                },
                needs_image,
            ),
        );
        props.insert("strength".into(), number(json!(1.0), 0.0, 1.0, false));
    }
    if needs_image {
        props
            .get_mut("input_images")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("default");
    }
    if mode == "inpaint" {
        required.extend(["input_images", "mask"]);
        props.insert("mask".into(),json!({"type":"string","description":"White pixels regenerate; black pixels preserve the source.","x-control":"mask_editor","x-source-field":"input_images","x-mask-format":"white-black"}));
        props.insert("mask_grow".into(), number(json!(0), 0.0, 256.0, true));
        props.insert(
            "mask_feather".into(),
            number(
                d.get("mask_blur").cloned().unwrap_or(json!(2.5)),
                0.0,
                64.0,
                false,
            ),
        );
    }
    if mode == "outpaint" {
        required.push("input_images");
        for key in [
            "outpaint_left",
            "outpaint_right",
            "outpaint_top",
            "outpaint_bottom",
        ] {
            let mut s = number(json!(128), 0.0, 2048.0, true);
            s["multipleOf"] = json!(64);
            props.insert(key.into(), s);
        }
    }
    props.insert("loras".into(),json!({"type":"array","default":[],"maxItems":10,"x-control":"lora_picker","items":{"type":"object","required":["path"],"properties":{
        "path":{"type":"string","enum":compatible(&catalog.loras, &profile.version),"x-accept-upload":{"extensions":[".safetensors"],"max_size":2147483648u64}},
        "weight":{"type":"number","minimum":-4,"maximum":4,"default":1.0},"mode":{"type":"string","enum":["all","base","refiner"],"default":"all"}}}}));
    if !profile.video {
        let mut upscalers = vec![String::new()];
        upscalers.extend(names(&catalog.upscalers));
        props.insert(
            "upscaler".into(),
            json!({"type":"string","default":"","enum":upscalers,"x-control":"dropdown"}),
        );
        props.insert(
            "upscaler_scale_factor".into(),
            number(json!(0), 0.0, 4.0, true),
        );
    }
    props.insert("control_images".into(), image_array(4, false));
    props.insert("controls".into(),json!({"type":"array","default":[],"maxItems":4,"description":"One control per control_images entry, in the same order. Supply preprocessed control images.","items":{"type":"object","required":["path"],"additionalProperties":false,"properties":{
        "path":{"type":"string","enum":compatible(&catalog.control_nets, &profile.version)},"hint_type":{"type":"string","enum":["custom","depth","canny","scribble","pose","normalbae","color","lineart","softedge","seg","inpaint","ip2p","shuffle","mlsd","tile","blur","lowquality","gray"]},
        "weight":{"type":"number","default":1.0,"minimum":0,"maximum":2},"guidance_start":{"type":"number","default":0,"minimum":0,"maximum":1},"guidance_end":{"type":"number","default":1,"minimum":0,"maximum":1},"mode":{"type":"string","default":"balanced","enum":["balanced","prompt","control"]}}}}));
    if compatible(&catalog.control_nets, &profile.version).is_empty() {
        props.remove("controls");
        props.remove("control_images");
    }
    if profile.video {
        props.insert("duration".into(), number(json!(5.0), 0.125, 20.0, false));
        props.insert(
            "fps".into(),
            number(d.get("fps").cloned().unwrap_or(json!(24)), 1.0, 60.0, true),
        );
        props.insert("generate_audio".into(),json!({"type":"boolean","default":true,"description":"Include native returned audio in the video; this does not disable engine audio inference."}));
    }
    if profile.spec["refiner"] == true {
        let mut schema = number(d["refiner_start"].clone(), 0.0, 1.0, false);
        schema["description"] = json!(
            "Fraction of the sampling schedule where the matching low-noise expert takes over."
        );
        props.insert("refiner_start".into(), schema);
    }
    if profile.spec["restoration"] == true {
        required = vec!["input_images"];
        props.get_mut("prompt").unwrap()["default"] = json!("");
        for key in [
            "prompt",
            "negative_prompt",
            "guidance",
            "shift",
            "sampler",
            "steps",
            "strength",
            "loras",
            "controls",
            "control_images",
            "upscaler",
            "upscaler_scale_factor",
        ] {
            props.remove(key);
        }
    }
    if profile.spec["audio"] != true {
        props.remove("generate_audio");
    }
    let mut native: Value = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/native_schema.json"
    )))
    .expect("native schema");
    // An explicit per-profile escape hatch, never the whole engine configuration.
    native["properties"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| {
            let common = [
                "tiled_decoding",
                "decoding_tile_width",
                "decoding_tile_height",
                "decoding_tile_overlap",
                "tiled_diffusion",
                "diffusion_tile_width",
                "diffusion_tile_height",
                "diffusion_tile_overlap",
                "tea_cache",
                "tea_cache_start",
                "tea_cache_end",
                "tea_cache_threshold",
                "tea_cache_max_skip_steps",
            ];
            common.contains(&key.as_str())
                || (profile.id == "sdxl"
                    && [
                        "clip_skip",
                        "separate_clip_l",
                        "clip_l_text",
                        "separate_open_clip_g",
                        "open_clip_g_text",
                        "aesthetic_score",
                        "negative_aesthetic_score",
                        "zero_negative_prompt",
                        "crop_top",
                        "crop_left",
                        "negative_original_image_width",
                        "negative_original_image_height",
                    ]
                    .contains(&key.as_str()))
                || (profile.id.starts_with("ltx-")
                    && [
                        "hires_fix",
                        "hires_fix_start_width",
                        "hires_fix_start_height",
                        "hires_fix_strength",
                    ]
                    .contains(&key.as_str()))
                || (mode == "inpaint" && key == "preserve_original_after_inpaint")
        });
    native["default"] = json!({});
    native["description"]=json!("Advanced native Draw Things settings. Tile and hires dimensions use native 64-pixel units. These override profile defaults. Only settings selected for this tool are exposed.");
    props.insert("native_configuration".into(), native);
    if mode == "upscale" {
        required = vec!["input_images"];
        props.get_mut("prompt").unwrap()["default"] = json!("");
        props.get_mut("strength").unwrap()["default"] = json!(0.0);
        props.get_mut("strength").unwrap()["enum"] = json!([0.0]);
        props.get_mut("upscaler").unwrap()["default"] = json!("realesrgan_x2plus_f16.ckpt");
    }
    let id = if mode == "generate" {
        profile.id.clone()
    } else {
        format!("{}-{mode}", profile.id)
    };
    let mut tasks = vec![if profile.video {
        "text-to-video"
    } else {
        "text-to-image"
    }];
    if profile.spec["requires_image"] == true {
        tasks.clear();
    }
    if profile.image {
        tasks.push(if profile.video {
            "image-to-video"
        } else {
            "image-to-image"
        });
    }
    if mode == "inpaint" {
        tasks = vec!["inpaint-image"];
    } else if mode == "outpaint" {
        tasks = vec!["outpaint-image"];
    }
    if mode == "upscale" || profile.spec["restoration"] == true {
        tasks = vec!["upscale-image"];
    }
    let advanced: Vec<_> = props
        .keys()
        .filter(|k| {
            ![
                "prompt",
                "negative_prompt",
                "input_images",
                "mask",
                "width",
                "height",
                "duration",
                "fps",
                "generate_audio",
            ]
            .contains(&k.as_str())
        })
        .map(|k| json!({"name":k}))
        .collect();
    let main: Vec<_> = [
        "input_images",
        "mask",
        "prompt",
        "negative_prompt",
        "width",
        "height",
        "duration",
        "fps",
        "generate_audio",
    ]
    .into_iter()
    .filter(|k| props.contains_key(*k))
    .map(|k| json!({"name":k}))
    .collect();
    json!({"id":id,"name":if mode=="generate"{profile.name.clone()}else{format!("{} {mode}",profile.name)},"task_types":tasks,"description":"Generate locally with Draw Things; missing official checkpoints download on first use.","parameter_schema":{"type":"object","required":required,"additionalProperties":false,"properties":props},"output_schema":{"type":"object","required":["assets"],"properties":{"assets":{"type":"array","items":{"type":"object"}}}},"layout":[{"label":"Generation","params":main},{"label":"Advanced","collapsed":true,"params":advanced}]})
}
pub fn descriptors(catalog: &MetadataOverride) -> Value {
    let mut tools = vec![];
    for profile in profiles(catalog) {
        tools.push(descriptor(&profile, "generate", catalog));
        if profile.id == "sdxl" {
            tools.push(descriptor(&profile, "upscale", catalog));
        }
        if profile.inpaint {
            tools.push(descriptor(&profile, "inpaint", catalog));
            tools.push(descriptor(&profile, "outpaint", catalog));
        }
    }
    json!({"tools":tools})
}
pub fn prepare(
    tool: &str,
    params: &Value,
    catalog: &MetadataOverride,
) -> Result<(Profile, String, Value)> {
    let list = descriptors(catalog);
    let descriptor = list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == tool)
        .context("Unknown tool")?;
    let mut values = params.clone();
    ensure!(values.is_object(), "Parameters must be an object");
    // Accept the existing Stimma LoRA payload spelling as well as canonical STP.
    if let Some(loras) = values.get_mut("loras").and_then(Value::as_array_mut) {
        for lora in loras {
            if lora.get("path").is_none() {
                if let Some(value) = lora.get("lora").cloned() {
                    lora["path"] = value;
                    lora.as_object_mut().unwrap().remove("lora");
                }
            }
        }
    }
    validate(&descriptor["parameter_schema"], &values, "parameters")?;
    for (key, schema) in descriptor["parameter_schema"]["properties"]
        .as_object()
        .unwrap()
    {
        if values.get(key).is_none() {
            if let Some(default) = schema.get("default") {
                values[key] = default.clone();
            }
        }
    }
    let (family, mode) = if let Some(s) = tool.strip_suffix("-inpaint") {
        (s, "inpaint")
    } else if let Some(s) = tool.strip_suffix("-outpaint") {
        (s, "outpaint")
    } else if let Some(s) = tool.strip_suffix("-upscale") {
        (s, "upscale")
    } else {
        (tool, "generate")
    };
    let profile = profiles(catalog)
        .into_iter()
        .find(|p| p.id == family)
        .context("Missing profile")?;
    if profile.spec["restoration"] == true {
        values["prompt"] = json!("");
        for key in ["steps", "guidance", "sampler", "shift"] {
            values[key] = profile.defaults[key].clone();
        }
    }
    Ok((profile, mode.into(), values))
}
pub fn validate(schema: &Value, value: &Value, path: &str) -> Result<()> {
    if let Some(kind) = schema["type"].as_str() {
        ensure!(
            match kind {
                "object" => value.is_object(),
                "array" => value.is_array(),
                "string" => value.is_string(),
                "boolean" => value.is_boolean(),
                "integer" => value.is_i64() || value.is_u64(),
                "number" => value.is_number(),
                _ => true,
            },
            "{path}: expected {kind}"
        );
    }
    if let Some(choices) = schema["enum"].as_array() {
        ensure!(
            choices.contains(value),
            "{path}: value is not one of the available choices"
        );
    }
    if let Some(number) = value.as_f64() {
        for (key, valid) in [("minimum", true), ("maximum", false)] {
            if let Some(bound) = schema[key].as_f64() {
                ensure!(
                    if valid {
                        number >= bound
                    } else {
                        number <= bound
                    },
                    "{path}: outside allowed range"
                );
            }
        }
        if let Some(unit) = schema["multipleOf"].as_f64() {
            ensure!(
                (number / unit).fract().abs() < 1e-9,
                "{path}: must be a multiple of {unit}"
            );
        }
    }
    if let Some(map) = value.as_object() {
        for key in schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            ensure!(map.contains_key(key), "{path}: missing {key}");
        }
        for (key, val) in map {
            let sub = &schema["properties"][key];
            ensure!(
                !sub.is_null() || schema["additionalProperties"] != false,
                "{path}: unknown field {key}"
            );
            if !sub.is_null() {
                validate(sub, val, &format!("{path}.{key}"))?;
            }
        }
    }
    if let Some(items) = value.as_array() {
        if let Some(max) = schema["maxItems"].as_u64() {
            ensure!(items.len() <= max as usize, "{path}: too many items");
        }
        if let Some(min) = schema["minItems"].as_u64() {
            ensure!(items.len() >= min as usize, "{path}: too few items");
        }
        for (i, v) in items.iter().enumerate() {
            validate(&schema["items"], v, &format!("{path}[{i}]"))?;
        }
    }
    Ok(())
}
