use crate::generated::stimma_drawthings::_generated::config::{
    GenerationConfiguration, GenerationConfigurationArgs, SamplerType,
};
use crate::proto::{
    image_generation_service_client::ImageGenerationServiceClient, EchoRequest,
    ImageGenerationRequest, MetadataOverride,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};

pub type Client = ImageGenerationServiceClient<Channel>;

#[derive(Clone)]
pub struct Engine {
    pub endpoint: String,
    pub secret: Option<String>,
}

impl Engine {
    pub async fn connect(&self) -> Result<Client> {
        let channel = Endpoint::from_shared(self.endpoint.clone())?
            .connect_timeout(Duration::from_secs(3))
            .connect()
            .await?;
        Ok(Client::new(channel).max_decoding_message_size(256 * 1024 * 1024))
    }
    pub async fn catalog(&self) -> Result<MetadataOverride> {
        let mut client = self.connect().await?;
        let mut request = tonic::Request::new(EchoRequest {
            name: "stimma-drawthings".into(),
            shared_secret: self.secret.clone(),
        });
        request.set_timeout(Duration::from_secs(5));
        let reply = client.echo(request).await?.into_inner();
        ensure!(
            !reply.shared_secret_missing,
            "Draw Things rejected the shared secret"
        );
        reply
            .r#override
            .context("Enable Draw Things Model Browsing")
    }
}

pub fn models(catalog: &MetadataOverride) -> Result<Vec<Value>> {
    serde_json::from_slice(&catalog.models).context("Invalid Draw Things model catalog")
}

pub fn descriptors(catalog: &MetadataOverride) -> Result<Value> {
    let models = models(catalog)?;
    let mut tools = Vec::new();
    for (id, name, version, steps, guidance) in [
        ("z-image-turbo", "Z-Image Turbo", "z_image", 8, 1.0),
        ("sdxl", "SDXL", "sdxl_base_v0.9", 16, 5.0),
    ] {
        let files: Vec<&str> = models
            .iter()
            .filter(|m| m["version"] == version)
            .filter(|m| id != "z-image-turbo" || m.to_string().to_lowercase().contains("turbo"))
            .filter_map(|m| m["file"].as_str())
            .collect();
        if files.is_empty() {
            continue;
        }
        tools.push(json!({"id":id,"name":name,"task_types":["text-to-image"],
            "parameter_schema":{"type":"object","additionalProperties":false,"required":["prompt"],"properties":{
                "prompt":{"type":"string","x-control":"prompt_editor"},
                "negative_prompt":{"type":"string","default":"","x-control":"textarea"},
                "checkpoint":{"type":"string","enum":files,"default":files[0],"x-control":"dropdown"},
                "width":{"type":"integer","minimum":64,"maximum":2048,"multipleOf":64,"default":1024,"x-control":"resolution"},
                "height":{"type":"integer","minimum":64,"maximum":2048,"multipleOf":64,"default":1024,"x-control":"resolution"},
                "steps":{"type":"integer","minimum":1,"maximum":100,"default":steps,"x-control":"slider"},
                "guidance":{"type":"number","minimum":0,"maximum":30,"default":guidance,"x-control":"slider"},
                "seed":{"type":"integer","minimum":0,"maximum":4294967295u64,"default":0,"x-control":"seed"}
            }},"output_schema":{"type":"object","required":["assets"],"properties":{"assets":{"type":"array"}}},
            "layout":[{"label":"Generation","params":[{"name":"prompt"},{"name":"width"},{"name":"height"},{"name":"seed"}]},
              {"label":"Advanced","collapsed":true,"params":[{"name":"checkpoint"},{"name":"negative_prompt"},{"name":"steps"},{"name":"guidance"}]}]
        }));
    }
    Ok(json!({"tools":tools}))
}

pub fn generation(
    tool: &str,
    params: &Value,
    catalog: &MetadataOverride,
) -> Result<ImageGenerationRequest> {
    let tools = descriptors(catalog)?;
    let descriptor = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == tool)
        .context("Unknown or unavailable tool")?;
    let properties = &descriptor["parameter_schema"]["properties"];
    let map = params.as_object().context("Parameters must be an object")?;
    for key in map.keys() {
        ensure!(!properties[key].is_null(), "Unknown parameter: {key}");
    }
    let value = |key: &str| {
        params
            .get(key)
            .unwrap_or(&properties[key]["default"])
            .clone()
    };
    let integer = |key: &str| -> Result<u32> {
        let n = value(key)
            .as_u64()
            .context(format!("{key} must be an unsigned integer"))?;
        ensure!(
            n >= properties[key]["minimum"].as_u64().unwrap()
                && n <= properties[key]["maximum"].as_u64().unwrap(),
            "{key} out of range"
        );
        Ok(n as u32)
    };
    let (width, height, steps, seed) = (
        integer("width")?,
        integer("height")?,
        integer("steps")?,
        integer("seed")?,
    );
    ensure!(
        width % 64 == 0 && height % 64 == 0,
        "Dimensions must be multiples of 64"
    );
    let model = value("checkpoint")
        .as_str()
        .context("checkpoint must be a string")?
        .to_owned();
    ensure!(
        properties["checkpoint"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!(model)),
        "Checkpoint is not compatible with this tool"
    );
    let guidance = value("guidance")
        .as_f64()
        .context("guidance must be a number")?;
    ensure!((0.0..=30.0).contains(&guidance), "guidance out of range");
    let prompt = params["prompt"]
        .as_str()
        .context("prompt is required")?
        .to_owned();
    let negative = value("negative_prompt")
        .as_str()
        .context("negative_prompt must be a string")?
        .to_owned();
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let model = builder.create_string(&model);
    let args = GenerationConfigurationArgs {
        start_width: (width / 64) as u16,
        start_height: (height / 64) as u16,
        steps,
        seed,
        guidance_scale: guidance as f32,
        strength: 1.0,
        model: Some(model),
        sampler: if tool == "z-image-turbo" {
            SamplerType::UniPCTrailing
        } else {
            SamplerType::DPMPP2MAYS
        },
        shift: if tool == "z-image-turbo" { 3.0 } else { 1.0 },
        resolution_dependent_shift: false,
        clip_skip: if tool == "sdxl" { 2 } else { 1 },
        zero_negative_prompt: tool == "sdxl",
        original_image_width: width,
        original_image_height: height,
        target_image_width: width,
        target_image_height: height,
        negative_original_image_width: if tool == "sdxl" { 512 } else { width },
        negative_original_image_height: if tool == "sdxl" { 512 } else { height },
        ..Default::default()
    };
    let root = GenerationConfiguration::create(&mut builder, &args);
    builder.finish(root, None);
    Ok(ImageGenerationRequest {
        configuration: builder.finished_data().to_vec(),
        prompt,
        negative_prompt: negative,
        scale_factor: 1,
        r#override: Some(catalog.clone()),
        user: "stimma-drawthings".into(),
        device: 2,
        chunked: true,
        ..Default::default()
    })
}
