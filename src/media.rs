use crate::install::{self, Progress, Runtime};
use anyhow::{ensure, Context, Result};
use image::{imageops::FilterType, DynamicImage, ImageReader};
use std::path::{Path, PathBuf};

pub async fn asset(root: &Path, id: &str) -> Result<PathBuf> {
    install::safe_name(id)?;
    let root = tokio::fs::canonicalize(root).await?;
    let path = tokio::fs::canonicalize(root.join(id))
        .await
        .context("Input asset not found")?;
    ensure!(
        path.starts_with(&root),
        "Asset must remain inside the STP asset directory"
    );
    Ok(path)
}
pub fn load(path: &Path) -> Result<DynamicImage> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    use image::ImageDecoder;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}
pub async fn input(root: &Path, id: &str, width: u32, height: u32) -> Result<image::RgbImage> {
    let path = asset(root, id).await?;
    tokio::task::spawn_blocking(move || {
        Ok(load(&path)?
            .resize_exact(width, height, FilterType::Lanczos3)
            .to_rgb8())
    })
    .await?
}
pub async fn mask(root: &Path, id: &str, width: u32, height: u32) -> Result<image::GrayImage> {
    let path = asset(root, id).await?;
    tokio::task::spawn_blocking(move || {
        Ok(load(&path)?
            .resize_exact(width, height, FilterType::Triangle)
            .to_luma8())
    })
    .await?
}
pub async fn video(
    runtime: &Runtime,
    dir: &Path,
    frames: usize,
    fps: u32,
    audio: &[(usize, Vec<f32>)],
    progress: &Progress,
) -> Result<PathBuf> {
    ensure!(
        frames > 0 && fps > 0,
        "No video frames or invalid frame rate"
    );
    let encoder = runtime.encoder(Some(progress)).await?;
    let output = dir.join("output.mp4");
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-framerate".into(),
        fps.to_string(),
        "-i".into(),
        dir.join("frame-%06d.png").to_string_lossy().into_owned(),
    ];
    if !audio.is_empty() {
        let mut pcm = vec![];
        for (channels, values) in audio {
            let samples = values.len() / channels;
            for i in 0..samples {
                for ch in 0..2 {
                    let v = values[i + ch.min(channels - 1) * samples];
                    pcm.extend(((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
                }
            }
        }
        ensure!(
            pcm.len() < u32::MAX as usize - 36,
            "Audio is too large for WAV"
        );
        let len = pcm.len() as u32;
        let mut wav = b"RIFF".to_vec();
        wav.extend((36 + len).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(48000u32.to_le_bytes());
        wav.extend(192000u32.to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(len.to_le_bytes());
        wav.extend(pcm);
        let path = dir.join("audio.wav");
        tokio::fs::write(&path, wav).await?;
        args.extend([
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-af".into(),
            "apad".into(),
            "-t".into(),
            format!("{:.9}", frames as f64 / fps as f64),
            "-c:a".into(),
            "aac".into(),
        ]);
    }
    args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.to_string_lossy().into_owned(),
    ]);
    progress.report(0.98, "Encoding video").await;
    let result = tokio::process::Command::new(encoder)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await?;
    ensure!(
        result.status.success(),
        "Video encoding failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(output)
}
