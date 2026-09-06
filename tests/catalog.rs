use serde_json::{json, Value};
use stimma_drawthings::{catalog, generation, proto::MetadataOverride};

fn metadata() -> MetadataOverride {
    let base: Value = serde_json::from_str(include_str!("../data/curated_models.json")).unwrap();
    let addons: Value = serde_json::from_str(include_str!("../data/addons.json")).unwrap();
    MetadataOverride {
        models: serde_json::to_vec(&base["models"]).unwrap(),
        loras: serde_json::to_vec(&addons["loras"]).unwrap(),
        control_nets: serde_json::to_vec(&addons["control_nets"]).unwrap(),
        upscalers: serde_json::to_vec(&addons["upscalers"]).unwrap(),
        ..Default::default()
    }
}
#[test]
fn all_named_tools_have_valid_defaults_and_pack_configuration() {
    let meta = metadata();
    let descriptors = catalog::descriptors(&meta);
    let tools = descriptors["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 28);
    for t in tools {
        let id = t["id"].as_str().unwrap();
        assert!(!id.starts_with("native-"));
        let schema = &t["parameter_schema"];
        for (name, field) in schema["properties"].as_object().unwrap() {
            if let Some(default) = field.get("default") {
                // Required source images deliberately have no usable empty default.
                if name != "input_images" || field["minItems"] == 0 {
                    catalog::validate(field, default, name).unwrap_or_else(|e| panic!("{id}: {e}"));
                }
            }
        }
        let mut input = json!({});
        if schema["properties"].get("prompt").is_some() {
            input["prompt"] = json!("schema fixture");
        }
        if schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "input_images")
        {
            input["input_images"] = json!(["source.png"]);
        }
        if schema["properties"].get("mask").is_some() {
            input["mask"] = json!("mask.png");
        }
        let (profile, _, params) = catalog::prepare(id, &input, &meta).unwrap();
        generation::configuration(&profile, &params, input.get("input_images").is_some())
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    for old in [
        "native-image",
        "native-image-inpaint",
        "native-image-outpaint",
        "native-video",
    ] {
        assert!(catalog::prepare(old, &json!({"prompt":"test"}), &meta).is_err());
    }
}
#[test]
fn model_specific_guidance_inputs_and_expert_pair_survive_wire() {
    let meta = metadata();
    let (profile, _, params) = catalog::prepare(
        "flux1-dev",
        &json!({"prompt":"test", "guidance_embed":4.5}),
        &meta,
    )
    .unwrap();
    let (wire, _) = generation::configuration(&profile, &params, false).unwrap();
    use stimma_drawthings::generated::stimma_drawthings::_generated::config::root_as_generation_configuration as decode;
    let config = decode(&wire).unwrap();
    assert_eq!(config.guidance_scale(), 1.0);
    assert_eq!(config.guidance_embed(), 4.5);
    assert!(catalog::prepare("flux1-dev", &json!({"prompt":"test", "guidance":4}), &meta).is_err());
    for id in [
        "qwen-edit-2509",
        "qwen-edit-2511",
        "wan22-i2v",
        "ltx-2.3-i2v",
        "seedvr2-3b",
    ] {
        assert!(
            catalog::prepare(id, &json!({"prompt":"test"}), &meta).is_err(),
            "{id}"
        );
        assert!(catalog::prepare(id, &json!({"prompt":"test", "input_images":[]}), &meta).is_err());
    }
    let (profile, _, params) =
        catalog::prepare("wan22-t2v", &json!({"prompt":"test"}), &meta).unwrap();
    let (wire, _) = generation::configuration(&profile, &params, false).unwrap();
    let config = decode(&wire).unwrap();
    assert_eq!(config.num_frames(), 81);
    assert_eq!(
        config.refiner_model(),
        Some("wan_v2.2_a14b_lne_t2v_q8p.ckpt")
    );
    assert_eq!(config.refiner_start(), 0.5);
    let (profile, _, params) =
        catalog::prepare("seedvr2-3b", &json!({"input_images":["source.png"]}), &meta).unwrap();
    let (wire, _) = generation::configuration(&profile, &params, true).unwrap();
    assert_eq!(decode(&wire).unwrap().num_frames(), 1);
    assert_eq!(decode(&wire).unwrap().steps(), 1);
}
#[test]
fn lora_and_control_choices_are_family_specific() {
    let mut meta = metadata();
    meta.loras = serde_json::to_vec(&json!([
        {"file":"sdxl-lora.ckpt","version":"sdxl_base_v0.9"},
        {"file":"flux-lora.ckpt","version":"flux1"}
    ]))
    .unwrap();
    let tools = catalog::descriptors(&meta);
    for t in tools["tools"].as_array().unwrap() {
        let props = &t["parameter_schema"]["properties"];
        assert!(props["native_configuration"]["properties"]
            .get("num_frames")
            .is_none());
        if t["id"] == "sdxl" {
            assert_eq!(
                props["loras"]["items"]["properties"]["path"]["enum"],
                json!(["sdxl-lora.ckpt"])
            );
        }
        if t["id"] == "flux1-dev" {
            assert_eq!(
                props["loras"]["items"]["properties"]["path"]["enum"],
                json!(["flux-lora.ckpt"])
            );
        }
        if t["id"] == "wan22-t2v" {
            assert!(props.get("generate_audio").is_none());
        }
        if t["id"] == "seedvr2-3b" {
            assert!(props.get("loras").is_none());
        }
    }
}

#[test]
fn embedded_models_and_expert_dependencies_have_pinned_checksums() {
    let base: Value = serde_json::from_str(include_str!("../data/curated_models.json")).unwrap();
    for model in base["models"].as_array().unwrap() {
        for file in stimma_drawthings::store::dependencies(model).unwrap() {
            let hash = base["sha256"][&file]
                .as_str()
                .unwrap_or_else(|| panic!("Missing checksum: {file}"));
            assert_eq!(hash.len(), 64, "{file}");
        }
    }
}
