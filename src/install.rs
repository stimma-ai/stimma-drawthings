//! Verified, atomic on-demand runtime components. No large payloads in the adapter.
use crate::engine::Engine;
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, Mutex},
};

#[derive(Clone)]
pub struct Progress {
    pub tx: mpsc::Sender<Value>,
    pub id: String,
}
impl Progress {
    pub async fn report(&self, p: f64, status: &str) {
        let _=self.tx.send(json!({"jsonrpc":"2.0","method":"tools.progress","params":{"request_id":self.id,"progress":p.clamp(0.0,1.0),"status":status}})).await;
    }
}

pub fn safe_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 255
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\', ':'])
            && !name.chars().any(char::is_control),
        "Expected a safe file basename"
    );
    Ok(())
}
pub fn state_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("STIMMA_DRAWTHINGS_STATE") {
        return p.into();
    }
    let home = PathBuf::from(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .unwrap_or_default(),
    );
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/ai.stimma.drawthings-provider")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or(home.join(".local/share"))
            .join("stimma-drawthings")
    }
}
pub fn desktop_models() -> Result<PathBuf> {
    ensure!(
        cfg!(target_os = "macos"),
        "Set --models-dir for a managed engine outside macOS"
    );
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is unavailable")?)
            .join("Library/Containers/com.liuliu.draw-things/Data/Documents/Models"),
    )
}
pub async fn digest(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
pub async fn lock(path: &Path) -> Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(200)).await
            }
            Err(e) => return Err(e.into()),
        }
    }
}
pub fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("stimma-drawthings/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(120))
        .build()?)
}

pub async fn fetch(
    url: &str,
    sha: &str,
    dest: &Path,
    progress: Option<&Progress>,
    offline: bool,
) -> Result<PathBuf> {
    ensure!(
        sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "Missing published SHA-256"
    );
    let parent = dest.parent().context("Download needs a parent directory")?;
    tokio::fs::create_dir_all(parent).await?;
    let _lock = lock(&dest.with_extension("lock")).await?;
    if dest.is_file() && digest(dest).await? == sha {
        return Ok(dest.to_owned());
    }
    ensure!(
        !offline,
        "Required component is not cached; offline mode prevents download"
    );
    if let Some(p) = progress {
        p.report(0.0, "Downloading required component").await;
    }
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    let mut response = http()?.get(url).send().await?.error_for_status()?;
    let expected = response.content_length();
    if let Some(size) = expected {
        ensure!(
            fs2::available_space(parent)? > size + 512 * 1024 * 1024,
            "Insufficient disk space for download"
        );
    }
    let mut hash = Sha256::new();
    let mut written = 0u64;
    let mut last = std::time::Instant::now();
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        hash.update(&chunk);
        written += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(400) {
            if let Some(p) = progress {
                p.report(
                    expected
                        .map(|n| written as f64 / n.max(1) as f64 * 0.2)
                        .unwrap_or(0.0),
                    "Downloading required component",
                )
                .await;
            }
            last = std::time::Instant::now();
        }
    }
    ensure!(
        expected.is_none_or(|n| n == written),
        "Download size mismatch"
    );
    ensure!(
        hex::encode(hash.finalize()) == sha,
        "SHA-256 mismatch; download discarded"
    );
    file.sync_all().await?;
    drop(file);
    temporary.persist(dest).map_err(|e| e.error)?;
    Ok(dest.to_owned())
}
fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

pub struct Runtime {
    pub engine: Engine,
    pub state: PathBuf,
    pub models: Option<PathBuf>,
    pub binary: Option<PathBuf>,
    pub managed: bool,
    pub offline: bool,
    pub converter: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub child: Mutex<Option<tokio::process::Child>>,
}
impl Runtime {
    pub async fn ensure(&self, progress: Option<&Progress>) -> Result<()> {
        if !self.managed {
            self.engine.catalog().await?;
            return Ok(());
        }
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            if child.try_wait()?.is_none() {
                return Ok(());
            }
            *guard = None;
        }
        let binary = if let Some(p) = &self.binary {
            p.clone()
        } else {
            ensure!(cfg!(target_os="macos"),"Automatic engine installation currently supports macOS; use --engine or --endpoint elsewhere");
            let p = self.state.join("runtime/v1.20260716.0/gRPCServerCLI-macOS");
            fetch("https://github.com/drawthingsai/draw-things-community/releases/download/v1.20260716.0/gRPCServerCLI-macOS","dd2084671b195afb07665b1f1956e8c774836008d43fbbb3e339534dee7ce0ee",&p,progress,self.offline).await?;
            executable(&p)?;
            p
        };
        let models = if let Some(p) = &self.models {
            p.clone()
        } else {
            desktop_models()?
        };
        tokio::fs::create_dir_all(&models).await?;
        let url = reqwest::Url::parse(&self.engine.endpoint)?;
        ensure!(
            url.scheme() == "http" && url.host_str() == Some("127.0.0.1"),
            "Managed engine must use an HTTP loopback endpoint"
        );
        let port = url
            .port()
            .context("Managed engine endpoint needs an explicit port")?;
        ensure!(
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err(),
            "Engine port is occupied; attach with --endpoint instead"
        );
        if let Some(p) = progress {
            p.report(0.0, "Starting Draw Things").await;
        }
        let mut command = tokio::process::Command::new(binary);
        command.arg(models).args([
            "--address",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--name",
            "Stimma Draw Things",
            "--no-tls",
            "--no-response-compression",
            "--model-browser",
        ]);
        if let Some(secret) = &self.engine.secret {
            command.args(["--shared-secret", secret]);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Starting Draw Things engine")?;
        for mut pipe in [
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
        ]
        .into_iter()
        .flatten()
        {
            // Drain verbose engine logs without exposing private paths or hostnames to STP.
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut pipe, &mut tokio::io::sink()).await;
            });
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            if self.engine.catalog().await.is_ok() {
                *guard = Some(child);
                return Ok(());
            }
            if let Some(status) = child.try_wait()? {
                bail!("Draw Things engine exited during startup ({status})");
            }
            ensure!(
                std::time::Instant::now() < deadline,
                "Draw Things startup timed out"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    pub async fn stop(&self) -> Result<()> {
        if let Some(mut child) = self.child.lock().await.take() {
            if child.try_wait()?.is_none() {
                child.kill().await?;
            }
            child.wait().await?;
        }
        Ok(())
    }
    pub async fn converter(&self, progress: Option<&Progress>) -> Result<PathBuf> {
        if let Some(p) = &self.converter {
            return Ok(p.clone());
        }
        ensure!(
            cfg!(all(target_os = "macos", target_arch = "aarch64")),
            "Set --lora-converter to a compatible Draw Things LoRAConverter on this platform"
        );
        let p = self.state.join("runtime/converter-eb4bc7f/LoRAConverter");
        fetch("https://github.com/stimma-ai/stimma-drawthings/releases/download/runtime-v1/LoRAConverter-macos-arm64","63e3086a8d72635b46a1f1b5f918b254a6bab7d48222c1d0f3418d5d20be74d4",&p,progress,self.offline).await?;
        executable(&p)?;
        Ok(p)
    }
    /// FFmpeg is never downloaded: use `--ffmpeg`, `STIMMA_DRAWTHINGS_FFMPEG`,
    /// or the `ffmpeg` on PATH that the host already asked the user to install.
    pub async fn encoder(&self, _progress: Option<&Progress>) -> Result<PathBuf> {
        if let Some(p) = &self.ffmpeg {
            return Ok(p.clone());
        }
        if let Some(p) = std::env::var_os("STIMMA_DRAWTHINGS_FFMPEG").filter(|v| !v.is_empty()) {
            let p = PathBuf::from(p);
            ensure!(
                p.is_file(),
                "STIMMA_DRAWTHINGS_FFMPEG does not point to an executable"
            );
            return Ok(p);
        }
        let name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
            .context("FFmpeg is not installed; install it or pass --ffmpeg")
    }
}
