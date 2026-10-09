//! Screen capture (cross-platform via xcap).

use image::RgbaImage;

use crate::{Error, MonitorInfo, Result};

pub struct MonitorShot {
    pub info: MonitorInfo,
    pub image: RgbaImage,
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Capture(e.to_string())
}

/// Capture every monitor. Used to show a frozen screen while the user selects a region.
pub fn capture_all() -> Result<Vec<MonitorShot>> {
    let monitors = xcap::Monitor::all().map_err(err)?;
    let mut shots = Vec::with_capacity(monitors.len());
    for (i, m) in monitors.iter().enumerate() {
        let info = MonitorInfo {
            index: i as u32,
            x: m.x().map_err(err)?,
            y: m.y().map_err(err)?,
            width: m.width().map_err(err)?,
            height: m.height().map_err(err)?,
            scale_factor: m.scale_factor().map_err(err)? as f64,
            is_primary: m.is_primary().unwrap_or(false),
        };
        let image = m.capture_image().map_err(err)?;
        shots.push(MonitorShot { info, image });
    }
    Ok(shots)
}

/// Encode as BMP: no compression, so it is instant even for 4K screens.
pub fn encode_bmp(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::with_capacity(img.as_raw().len() + 256));
    img.write_to(&mut out, image::ImageFormat::Bmp).map_err(err)?;
    Ok(out.into_inner())
}

pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(err)?;
    Ok(out.into_inner())
}

/// Crop a region given in physical pixels relative to the image.
pub fn crop(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    let x = x.min(img.width().saturating_sub(1));
    let y = y.min(img.height().saturating_sub(1));
    let w = w.min(img.width() - x).max(1);
    let h = h.min(img.height() - y).max(1);
    image::imageops::crop_imm(img, x, y, w, h).to_image()
}
