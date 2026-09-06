//! Draw Things' uncompressed NNC envelope. No native imaging runtime required.
use anyhow::{bail, ensure, Context, Result};
use half::f16;
use image::{DynamicImage, ImageBuffer, ImageFormat};
use std::io::Cursor;

pub fn pngs(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    ensure!(data.len() >= 68, "Truncated tensor header");
    let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
    ensure!(word(0) == 0, "Disable Draw Things Response Compression");
    ensure!(word(2) == 2, "Expected NHWC image tensor");
    let mut shape = Vec::new();
    let mut ended = false;
    for i in 5..17 {
        let d = word(i);
        if d == 0 {
            ended = true;
        } else {
            ensure!(!ended && d <= i32::MAX as u32, "Invalid tensor dimensions");
            shape.push(d as usize);
        }
    }
    if shape.len() == 3 {
        shape.insert(0, 1);
    }
    ensure!(
        shape.len() == 4 && matches!(shape[3], 3 | 4),
        "Expected NHWC RGB/RGBA tensor"
    );
    let size = match word(3) {
        0x1000 => 1,
        0x4000 => 4,
        0x20000 => 2,
        _ => bail!("Unsupported tensor datatype"),
    };
    let count = shape
        .iter()
        .try_fold(1usize, |n, &d| n.checked_mul(d))
        .context("Tensor dimension overflow")?;
    ensure!(
        count.checked_mul(size) == Some(data.len() - 68),
        "Tensor payload size mismatch"
    );
    let pixels: Vec<u8> = data[68..]
        .chunks_exact(size)
        .map(|p| {
            if size == 1 {
                return p[0];
            }
            let v = if size == 2 {
                f16::from_le_bytes(p.try_into().unwrap()).to_f32()
            } else {
                f32::from_le_bytes(p.try_into().unwrap())
            };
            ((v + 1.0) * 127.5).clamp(0.0, 255.0) as u8
        })
        .collect();
    let frame_size = count / shape[0];
    pixels
        .chunks_exact(frame_size)
        .map(|frame| {
            let (w, h) = (shape[2] as u32, shape[1] as u32);
            let image = if shape[3] == 3 {
                DynamicImage::ImageRgb8(
                    ImageBuffer::from_raw(w, h, frame.to_vec()).context("Invalid RGB shape")?,
                )
            } else {
                DynamicImage::ImageRgba8(
                    ImageBuffer::from_raw(w, h, frame.to_vec()).context("Invalid RGBA shape")?,
                )
            };
            let mut out = Cursor::new(Vec::new());
            image.write_to(&mut out, ImageFormat::Png)?;
            Ok(out.into_inner())
        })
        .collect()
}

pub fn encode_image(image: &image::RgbImage) -> Vec<u8> {
    let mut words = [0u32; 17];
    words[1] = 1;
    words[2] = 2;
    words[3] = 0x20000;
    words[5..9].copy_from_slice(&[1, image.height(), image.width(), 3]);
    let mut out: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    out.reserve(image.len() * 2);
    for pixel in image.as_raw() {
        out.extend(half::f16::from_f32(*pixel as f32 / 127.5 - 1.0).to_le_bytes());
    }
    out
}
pub fn encode_mask(image: &image::GrayImage, scribble: bool) -> Vec<u8> {
    let mut words = [0u32; 17];
    words[1] = 1;
    words[2] = 1;
    words[3] = 0x1000;
    words[5..7].copy_from_slice(&[image.height(), image.width()]);
    let mut out: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    out.extend(image.as_raw().iter().map(|p| {
        if scribble {
            if *p < 128 {
                0
            } else {
                255
            }
        } else if *p >= 128 {
            2
        } else {
            0
        }
    }));
    out
}
pub fn audio(data: &[u8]) -> Result<(usize, Vec<f32>)> {
    ensure!(data.len() >= 68, "Truncated audio tensor");
    let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
    ensure!(word(0) == 0, "Compressed audio tensor is unsupported");
    let shape: Vec<_> = (5..17).map(word).take_while(|v| *v != 0).collect();
    ensure!(
        !shape.is_empty() && shape.len() <= 2,
        "Expected channel-first audio tensor"
    );
    let (channels, samples) = if shape.len() == 1 {
        (1, shape[0] as usize)
    } else {
        (shape[0] as usize, shape[1] as usize)
    };
    ensure!((1..=2).contains(&channels), "Expected mono or stereo audio");
    let size = match word(3) {
        0x20000 => 2,
        0x4000 => 4,
        _ => bail!("Unsupported audio datatype"),
    };
    ensure!(
        channels
            .checked_mul(samples)
            .and_then(|n| n.checked_mul(size))
            == Some(data.len() - 68),
        "Audio payload size mismatch"
    );
    let values = data[68..]
        .chunks_exact(size)
        .map(|b| {
            if size == 2 {
                half::f16::from_le_bytes(b.try_into().unwrap()).to_f32()
            } else {
                f32::from_le_bytes(b.try_into().unwrap())
            }
        })
        .collect();
    Ok((channels, values))
}

/// Lightweight latent preview, using ComfyUI's published RGB projection matrices.
/// Only known image latent formats are rendered; these are approximate previews.
pub fn preview(data: &[u8], version: &str) -> Result<Vec<u8>> {
    ensure!(
        data.len() >= 68 && data.len() <= 16 * 1024 * 1024,
        "Invalid preview size"
    );
    let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
    ensure!(word(0) == 0 && word(2) == 2, "Unsupported preview envelope");
    let (batch, h, w, c) = (word(5), word(6), word(7), word(8));
    ensure!(
        batch > 0 && h > 0 && w > 0 && h <= 512 && w <= 512 && word(9) == 0,
        "Unsupported preview dimensions"
    );
    let size = match word(3) {
        0x20000 => 2,
        0x4000 => 4,
        _ => bail!("Unsupported preview datatype"),
    };
    ensure!(
        batch
            .checked_mul(h)
            .and_then(|n| n.checked_mul(w))
            .and_then(|n| n.checked_mul(c))
            .and_then(|n| n.checked_mul(size))
            == Some(data.len() - 68),
        "Invalid preview payload"
    );
    let name = match version {
        "v1" | "v2" => "SD15",
        "sdxl_base_v0.9" | "sdxl_refiner_v0.9" | "ssd_1b" => "SDXL",
        "flux1" | "z_image" => "Flux",
        "flux2" | "flux2_9b" | "flux2_4b" => "Flux2",
        _ => bail!("No preview projection for model"),
    };
    let matrices: serde_json::Value = serde_json::from_str(include_str!("../data/preview.json"))?;
    let matrix = matrices[name]["latent_rgb_factors"]
        .as_array()
        .context("Missing preview matrix")?;
    ensure!(matrix.len() == c, "Unsupported latent channel layout");
    let bias = &matrices[name]["latent_rgb_factors_bias"];
    let mut pixels = Vec::with_capacity(h * w * 3);
    for latent in data[68..68 + h * w * c * size].chunks_exact(c * size) {
        let mut rgb = [0f32; 3];
        for channel in 0..3 {
            rgb[channel] = bias[channel].as_f64().unwrap_or(0.0) as f32;
        }
        for (i, b) in latent.chunks_exact(size).enumerate() {
            let v = if size == 2 {
                f16::from_le_bytes(b.try_into().unwrap()).to_f32()
            } else {
                f32::from_le_bytes(b.try_into().unwrap())
            };
            for channel in 0..3 {
                rgb[channel] += v * matrix[i][channel].as_f64().unwrap() as f32;
            }
        }
        pixels.extend(rgb.map(|v| ((v + 1.0) * 127.5).clamp(0.0, 255.0) as u8));
    }
    let image = DynamicImage::ImageRgb8(
        ImageBuffer::from_raw(w as u32, h as u32, pixels).context("Invalid preview pixels")?,
    );
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png)?;
    Ok(png.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tensor() -> Vec<u8> {
        let mut words = [0u32; 17];
        words[1] = 1;
        words[2] = 2;
        words[3] = 0x20000;
        words[5..9].copy_from_slice(&[1, 1, 1, 3]);
        let mut bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        for v in [-1.0, 1.0, -1.0] {
            bytes.extend(f16::from_f32(v).to_le_bytes());
        }
        bytes
    }
    #[test]
    fn decodes_half_float_green_pixel() {
        let png = pngs(&tensor()).unwrap().remove(0);
        assert_eq!(
            image::load_from_memory(&png).unwrap().to_rgb8().as_raw(),
            &[0, 255, 0]
        );
    }
    #[test]
    fn rejects_compressed_and_truncated_tensors() {
        let mut bytes = tensor();
        bytes[0] = 1;
        assert!(pngs(&bytes).is_err());
        bytes[0] = 0;
        bytes.pop();
        assert!(pngs(&bytes).is_err());
    }
}
