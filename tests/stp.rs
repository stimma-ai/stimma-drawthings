//! The real STP CLI talks to the native binary and a deterministic gRPC engine.
use serde_json::{json, Value};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use stimma_drawthings::{
    catalog as tools_catalog, generation,
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
        let request = r.into_inner();
        let slow = request.prompt == "slow";
        self.requests.lock().unwrap().push(request);
        if slow {
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
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
        r: Request<proto::FileListRequest>,
    ) -> Result<Response<proto::FileExistenceResponse>, Status> {
        let files = r.into_inner().files;
        Ok(Response::new(proto::FileExistenceResponse {
            existences: vec![true; files.len()],
            files,
            hashes: vec![],
        }))
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
    let (profile, _, params) = tools_catalog::prepare(
        "z-image-turbo",
        &json!({"prompt":"green","width":512,"height":768,"seed":7}),
        &catalog(),
    )
    .unwrap();
    let (wire, _) = generation::configuration(&profile, &params, false).unwrap();
    let config=stimma_drawthings::generated::stimma_drawthings::_generated::config::root_as_generation_configuration(&wire).unwrap();
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
        assert!(tools_catalog::prepare("z-image-turbo", &params, &catalog()).is_err());
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
        "'{}' --offline --state-path '{}' --endpoint http://127.0.0.1:{port}",
        env!("CARGO_BIN_EXE_stimma-drawthings"),
        dir.path().display()
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

#[tokio::test]
async fn websocket_stp_and_authenticated_asset_transfer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(ImageGenerationServiceServer::new(Mock::default()))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let web = reserve.local_addr().unwrap();
    drop(reserve);
    let dir = tempfile::tempdir().unwrap();
    let mut provider = tokio::process::Command::new(env!("CARGO_BIN_EXE_stimma-drawthings"))
        .args([
            "--websocket",
            "--offline",
            "--bind",
            &web.to_string(),
            "--endpoint",
            &format!("http://127.0.0.1:{port}"),
            "--state-path",
        ])
        .arg(dir.path())
        .env("STIMMA_DRAWTHINGS_TOKEN", "test-token")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(web).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let client = reqwest::Client::new();
    let asset = format!("http://{web}/assets/fixture.png");
    assert_eq!(
        client
            .put(&asset)
            .body("fixture")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert!(client
        .put(&asset)
        .bearer_auth("test-token")
        .body("fixture")
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    assert_eq!(
        client
            .get(&asset)
            .bearer_auth("test-token")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "fixture"
    );
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("stp")
            .args([
                "--url",
                &format!("ws://{web}/stp-v1"),
                "--token",
                "test-token",
                "run",
                "z-image-turbo",
                "green",
                "-o",
            ])
            .arg(dir.path().join("output.png"))
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
    assert_eq!(
        image::open(dir.path().join("output.png"))
            .unwrap()
            .to_rgb8()
            .get_pixel(0, 0)
            .0,
        [0, 255, 0]
    );
    provider.kill().await.unwrap();
    provider.wait().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn queued_job_cancellation_has_one_terminal_and_next_job_runs() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mock = Mock::default();
    let requests = mock.requests.clone();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(ImageGenerationServiceServer::new(mock))
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_stimma-drawthings"))
        .args([
            "--offline",
            "--endpoint",
            &format!("http://127.0.0.1:{port}"),
            "--state-path",
        ])
        .arg(dir.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let register = lines.next_line().await.unwrap().unwrap();
    assert!(register.contains("provider.register"));
    for (id, prompt) in [("first", "slow"), ("cancel", "unused"), ("last", "green")] {
        let message = json!({"jsonrpc":"2.0","id":id,"method":"tools.execute","params":{"request_id":id,"tool_id":"z-image-turbo","parameters":{"prompt":prompt}}});
        input
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }
    input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"c\",\"method\":\"tools.cancel\",\"params\":{\"request_id\":\"cancel\"}}\n").await.unwrap();
    input.flush().await.unwrap();
    let outcomes = tokio::time::timeout(Duration::from_secs(10), async {
        let mut terminal = Vec::new();
        let mut accepted = 0;
        while terminal.len() < 3 {
            let value: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            if value["result"]["accepted"] == true {
                accepted += 1;
            }
            if value["method"] == "tools.result" {
                terminal.push(value["params"].clone());
            }
        }
        assert_eq!(accepted, 3);
        terminal
    })
    .await
    .unwrap();
    assert_eq!(
        outcomes
            .iter()
            .filter(|v| v["request_id"] == "cancel")
            .count(),
        1
    );
    assert!(outcomes
        .iter()
        .any(|v| v["request_id"] == "cancel" && v["error"]["code"] == "CANCELLED"));
    assert!(outcomes
        .iter()
        .any(|v| v["request_id"] == "last" && v["success"] == true));
    assert_eq!(requests.lock().unwrap().len(), 2);
    drop(input);
    tokio::time::timeout(Duration::from_secs(3), child.wait())
        .await
        .unwrap()
        .unwrap();
    server.abort();
}

#[tokio::test]
async fn network_listener_works_without_auth_configuration() {
    let reserve = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = reserve.local_addr().unwrap().port();
    drop(reserve);
    let dir = tempfile::tempdir().unwrap();
    let mut provider = tokio::process::Command::new(env!("CARGO_BIN_EXE_stimma-drawthings"))
        .args([
            "--websocket",
            "--offline",
            "--bind",
            &format!("0.0.0.0:{port}"),
            "--state-path",
        ])
        .arg(dir.path())
        .env_remove("STIMMA_DRAWTHINGS_TOKEN")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        assert!(
            provider.try_wait().unwrap().is_none(),
            "Unauthenticated network listener exited"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("stp")
            .args([
                "--url",
                &format!("ws://127.0.0.1:{port}/stp-v1"),
                "raw",
                "tools.list",
            ])
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("z-image-turbo"));
    provider.kill().await.unwrap();
    provider.wait().await.unwrap();
}
