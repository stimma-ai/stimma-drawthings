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
