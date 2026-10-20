//! Reading source images for encoding and viewing. Shared by the `bic` and
//! `bic-view` binaries; the decoding itself lives in the library
//! (`bic::read_source`).

use anyhow::{Context, Result};
use bic::{read_source, DecodeLimits, Source};
use image::RgbaImage;
use std::{fs, path::Path};

/// File extensions `load` reads.
pub const EXTENSIONS: [&str; 11] = [
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "qoi", "heic", "heif",
];

/// A source image: upright 8-bit RGBA, its metadata, and the file it came
/// from (which a BIC file may store unchanged).
pub struct Loaded {
    pub image: RgbaImage,
    pub source: Source,
    pub original: Vec<u8>,
}

pub fn load(path: &Path) -> Result<Loaded> {
    let original = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut source = read_source(&original, DecodeLimits::default())
        .with_context(|| format!("decoding {}", path.display()))?;
    let rgba = std::mem::take(&mut source.rgba);
    let image =
        RgbaImage::from_raw(source.width, source.height, rgba).context("decoded size mismatch")?;
    Ok(Loaded {
        image,
        source,
        original,
    })
}
