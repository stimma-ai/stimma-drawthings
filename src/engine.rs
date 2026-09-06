use crate::proto::{
    image_generation_service_client::ImageGenerationServiceClient, EchoRequest, MetadataOverride,
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
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
        Ok(Client::new(channel)
            .max_decoding_message_size(1024 * 1024 * 1024)
            .max_encoding_message_size(1024 * 1024 * 1024))
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

impl Engine {
    pub async fn files_exist(
        &self,
        files: Vec<String>,
    ) -> Result<std::collections::HashMap<String, bool>> {
        for f in &files {
            crate::install::safe_name(f)?;
        }
        let mut r = tonic::Request::new(crate::proto::FileListRequest {
            files,
            shared_secret: self.secret.clone(),
            ..Default::default()
        });
        r.set_timeout(Duration::from_secs(30));
        let response = self.connect().await?.files_exist(r).await?.into_inner();
        ensure!(
            response.files.len() == response.existences.len(),
            "Invalid FilesExist response"
        );
        Ok(response
            .files
            .into_iter()
            .zip(response.existences)
            .collect())
    }
    pub async fn upload(
        &self,
        name: &str,
        path: &std::path::Path,
        p: Option<&crate::install::Progress>,
    ) -> Result<()> {
        use crate::proto::{file_upload_request, FileChunk, FileUploadRequest, InitUploadRequest};
        use tokio::io::AsyncReadExt;
        crate::install::safe_name(name)?;
        let hash = hex::decode(crate::install::digest(path).await?)?;
        let mut file = tokio::fs::File::open(path).await?;
        let size = file.metadata().await?.len();
        ensure!(size > 0 && size <= i64::MAX as u64, "Invalid upload size");
        let filename = name.to_owned();
        let secret = self.secret.clone();
        let requests = async_stream::stream! {
            yield FileUploadRequest{request:Some(file_upload_request::Request::InitRequest(InitUploadRequest{filename:filename.clone(),sha256:hash,total_size:size as i64})),shared_secret:secret.clone()};
            let mut offset=0u64;let mut buf=vec![0u8;4*1024*1024];
            while let Ok(n)=file.read(&mut buf).await {
                if n==0{break;}
                yield FileUploadRequest{request:Some(file_upload_request::Request::Chunk(FileChunk{filename:filename.clone(),content:buf[..n].to_vec(),offset:offset as i64})),shared_secret:secret.clone()};offset+=n as u64;
            }
        };
        let mut client = self.connect().await?;
        let mut response = client.upload_file(requests).await?.into_inner();
        let mut received = 0i64;
        while let Some(reply) = response.message().await? {
            ensure!(
                reply.chunk_upload_success,
                "Draw Things rejected upload: {}",
                reply.message
            );
            received = received.max(reply.received_offset);
            if let Some(p) = p {
                p.report(
                    0.2 + 0.05 * received as f64 / size as f64,
                    "Transferring model to Draw Things",
                )
                .await;
            }
        }
        ensure!(
            received >= size as i64,
            "Draw Things did not acknowledge the complete upload"
        );
        ensure!(
            self.files_exist(vec![name.into()]).await?.get(name) == Some(&true),
            "Uploaded file not found on engine"
        );
        Ok(())
    }
}
