//! Measurements that build tools use to decide whether an image should be
//! stored as BIC. The policy (thresholds, what counts as a photograph) lives
//! in the caller; this module only measures.

use crate::load::load;
use anyhow::{anyhow, Result};
use bic::{encode_source, storage, EncodeOptions, Storage};
use image::{codecs::webp::WebPEncoder, ExtendedColorType, ImageFormat};
use std::{collections::HashSet, path::Path};

const OXIPNG_PRESET: u8 = 2;
/// Counting stops here: past this many colors the image is a photograph for
/// any policy that cares, and the exact number no longer matters.
const COLOR_COUNT_CAP: usize = 1 << 17;

pub struct Report {
    pub width: u32,
    pub height: u32,
    /// Distinct RGBA colors, counted up to COLOR_COUNT_CAP.
    pub distinct_colors: usize,
    pub png: usize,
    pub oxipng: usize,
    pub webp_lossless: usize,
    /// The `.bic` file that `bic encode` writes with default settings.
    pub bic: usize,
    pub bic_storage: Storage,
}

pub fn inspect(path: &Path) -> Result<Report> {
    let loaded = load(path)?;
    let image = &loaded.image;
    let (width, height) = image.dimensions();

    let distinct_colors = count_colors(image.as_raw());

    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png)?;
    let png = png.into_inner();
    let oxipng = oxipng::optimize_from_memory(&png, &oxipng::Options::from_preset(OXIPNG_PRESET))
        .map_err(|e| anyhow!("oxipng: {e}"))?
        .len();

    let mut webp = Vec::new();
    WebPEncoder::new_lossless(&mut webp).encode(
        image.as_raw(),
        width,
        height,
        ExtendedColorType::Rgba8,
    )?;

    let mut source = loaded.source;
    source.rgba = loaded.image.into_raw();
    let bic = encode_source(&source, &loaded.original, EncodeOptions::default())?;

    Ok(Report {
        width,
        height,
        distinct_colors,
        png: png.len(),
        oxipng,
        webp_lossless: webp.len(),
        bic: bic.len(),
        bic_storage: storage(&bic)?,
    })
}

fn count_colors(rgba: &[u8]) -> usize {
    let mut seen: HashSet<[u8; 4]> = HashSet::new();
    for &px in rgba.as_chunks::<4>().0 {
        seen.insert(px);
        if seen.len() >= COLOR_COUNT_CAP {
            break;
        }
    }
    seen.len()
}

impl Report {
    /// JSON for scripts. Written by hand to avoid a serializer dependency.
    pub fn to_json(&self) -> String {
        let storage = match self.bic_storage {
            Storage::Native => "native",
            Storage::PngFallback => "png",
            Storage::Original => "original",
        };
        format!(
            "{{\"width\":{},\"height\":{},\"distinct_colors\":{},\"sizes\":{{\"png\":{},\"oxipng\":{},\"webp_lossless\":{},\"bic\":{}}},\"bic_storage\":\"{}\"}}",
            self.width,
            self.height,
            self.distinct_colors,
            self.png,
            self.oxipng,
            self.webp_lossless,
            self.bic,
            storage
        )
    }
}
