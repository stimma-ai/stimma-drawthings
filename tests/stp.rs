//! The real STP CLI talks to the native binary and a deterministic gRPC engine.
use serde_json::{json, Value};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use stimma_drawthings::{
    engine,
    proto::{
        self,
        image_generation_service_server::{ImageGenerationService, ImageGenerationServiceServer},
    },
};
use tokio_stream::{wrappers::TcpListenerStream, Stream};
use tonic::{Request, Response, Status};

#[derive(Clone, Default)]
struct Mock {
    requests: Arc<Mutex<Vec<proto::ImageGenerationRequest>>>,
}
type Replies<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

fn catalog() -> proto::MetadataOverride {
    proto::MetadataOverride {
        models:serde_json::to_vec(&json!([{"name":"Z Image Turbo Test","version":"z_image","file":"z_image_turbo_test.ckpt"}])).unwrap(),
        loras:b"[]".to_vec(),control_nets:b"[]".to_vec(),upscalers:b"[]".to_vec(),textual_inversions:b"[]".to_vec()
    }
}

#[tonic::async_trait]
impl ImageGenerationService for Mock {
    type GenerateImageStream = Replies<proto::ImageGenerationResponse>;
    type UploadFileStream = Replies<proto::UploadResponse>;
    async fn echo(
        &self,
        _: Request<proto::EchoRequest>,
    ) -> Result<Response<proto::EchoReply>, Status> {
        Ok(Response::new(proto::EchoReply {
            r#override: Some(catalog()),
            ..Default::default()
        }))
    }
    async fn generate_image(
        &self,
        r: Request<proto::ImageGenerationRequest>,
    ) -> Result<Response<Self::GenerateImageStream>, Status> {
        self.requests.lock().unwrap().push(r.into_inner());
        let mut words = [0u32; 17];
        words[1] = 1;
        words[2] = 2;
        words[3] = 0x1000;
        words[5..9].copy_from_slice(&[1, 2, 2, 3]);
        let mut bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        bytes.extend([0, 255, 0].repeat(4));
        let split = bytes.len() / 2;
        let replies = vec![
            Ok(proto::ImageGenerationResponse {
                generated_images: vec![bytes[..split].to_vec()],
                chunk_state: 1,
                ..Default::default()
            }),
            Ok(proto::ImageGenerationResponse {
                generated_images: vec![bytes[split..].to_vec()],
                chunk_state: 0,
                ..Default::default()
            }),
        ];
        Ok(Response::new(Box::pin(tokio_stream::iter(replies))))
    }
    async fn files_exist(
        &self,
        _: Request<proto::FileListRequest>,
    ) -> Result<Response<proto::FileExistenceResponse>, Status> {
        Err(Status::unimplemented("fixture"))
    }
    async fn upload_file(
        &self,
        _: Request<tonic::Streaming<proto::FileUploadRequest>>,
    ) -> Result<Response<Self::UploadFileStream>, Status> {
        Err(Status::unimplemented("fixture"))
    }
    async fn pubkey(
        &self,
        _: Request<proto::PubkeyRequest>,
    ) -> Result<Response<proto::PubkeyResponse>, Status> {
        Err(Status::unimplemented("fixture"))
    }
    async fn hours(
        &self,
        _: Request<proto::HoursRequest>,
    ) -> Result<Response<proto::HoursResponse>, Status> {
        Err(Status::unimplemented("fixture"))
    }
}

#[test]
fn configuration_validation_and_wire_units() {
    let request = engine::generation(
        "z-image-turbo",
        &json!({"prompt":"green","width":512,"height":768,"seed":7}),
        &catalog(),
    )
    .unwrap();
    let config=stimma_drawthings::generated::stimma_drawthings::_generated::config::root_as_generation_configuration(&request.configuration).unwrap();
    assert_eq!(
        (
            config.start_width(),
            config.start_height(),
            config.seed(),
            config.steps()
        ),
        (8, 12, 7, 8)
    );
    for params in [
        json!({"prompt":"x","width":513}),
        json!({"prompt":"x","seed":-1}),
        json!({"prompt":"x","typo":1}),
        json!({"prompt":"x","checkpoint":"other.ckpt"}),
    ] {
        assert!(engine::generation("z-image-turbo", &params, &catalog()).is_err());
    }
}

#[tokio::test]
async fn real_stp_cli_discovers_and_generates_png() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let service = Mock::default();
    let requests = service.requests.clone();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(ImageGenerationServiceServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    let dir = tempfile::tempdir().unwrap();
    let provider = format!(
        "'{}' --endpoint http://127.0.0.1:{port}",
        env!("CARGO_BIN_EXE_stimma-drawthings")
    );
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("stp")
            .args(["--exec", &provider, "tools", "--json"])
            .output(),
    )
    .await
    .unwrap()
    .expect("stp CLI is required for this integration test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("z-image-turbo"));
    let file = dir.path().join("result.png");
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("stp")
            .args([
                "--exec",
                &provider,
                "run",
                "z-image-turbo",
                "green square",
                "--width",
                "512",
                "--height",
                "512",
                "--seed",
                "7",
                "--json",
                "-o",
            ])
            .arg(&file)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let image = image::open(&file).unwrap().to_rgb8();
    assert_eq!(image.dimensions(), (2, 2));
    assert_eq!(image.get_pixel(0, 0).0, [0, 255, 0]);
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(response.to_string().contains("actual_seed"));
    assert_eq!(requests.lock().unwrap()[0].prompt, "green square");
    server.abort();
}
