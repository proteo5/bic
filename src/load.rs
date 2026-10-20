//! Reading source images for encoding and viewing. Shared by the `bic` and
//! `bic-view` binaries.

use anyhow::{Context, Result};
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use std::{fs, io::Cursor, path::Path};

/// File extensions `load` reads.
pub const EXTENSIONS: [&str; 11] = [
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "qoi", "heic", "heif",
];

/// A source image as BIC stores it: 8-bit RGBA, upright, plus its profile.
pub struct Loaded {
    pub image: RgbaImage,
    pub icc_profile: Option<Vec<u8>>,
    /// Conversions the caller may want to report, such as bit-depth loss.
    pub notes: Vec<String>,
}

pub fn load(path: &Path) -> Result<Loaded> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext == "heic" || ext == "heif" {
        return load_heic(&data);
    }
    let mut decoder = ImageReader::new(Cursor::new(&data))
        .with_guessed_format()?
        .into_decoder()
        .with_context(|| format!("decoding {}", path.display()))?;
    // Profiles and orientation are best effort: a broken tag should not make
    // an otherwise readable image fail to load.
    let icc_profile = decoder.icc_profile().ok().flatten();
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let mut notes = Vec::new();
    let color = image.color();
    if color.bytes_per_pixel() > color.channel_count() {
        notes.push("samples above 8 bits were reduced to 8 bits".into());
    }
    Ok(Loaded {
        image: image.to_rgba8(),
        icc_profile,
        notes,
    })
}

fn load_heic(data: &[u8]) -> Result<Loaded> {
    let options = heic_rs::DecodeOptions::default()
        .with_layout(heic_rs::PixelLayout::Rgba8)
        .with_alpha(true);
    // heic-rs applies the HEIF rotation and mirror properties itself.
    let decoded = heic_rs::decode(data, &options).context("decoding HEIC/HEIF")?;
    let icc_profile = heic_rs::context::Context::open(data)
        .ok()
        .and_then(|ctx| ctx.props(ctx.meta.primary).ok())
        .and_then(|props| props.icc.map(<[u8]>::to_vec));
    let mut notes = Vec::new();
    if heic_rs::probe(data).is_ok_and(|info| info.bit_depth > 8) {
        notes.push("samples above 8 bits were reduced to 8 bits".into());
    }
    Ok(Loaded {
        image: RgbaImage::from_raw(decoded.width, decoded.height, decoded.data)
            .context("invalid HEIC RGBA dimensions")?,
        icc_profile,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A JPEG with an EXIF APP1 segment whose Orientation tag is `value`.
    fn jpeg_with_orientation(img: &RgbaImage, value: u16) -> Vec<u8> {
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgba8(img.clone())
            .to_rgb8()
            .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        // Little-endian TIFF with one IFD entry: tag 0x0112, SHORT, count 1.
        let mut tiff = b"II*\0".to_vec();
        tiff.extend(8u32.to_le_bytes());
        tiff.extend(1u16.to_le_bytes());
        tiff.extend(0x0112u16.to_le_bytes());
        tiff.extend(3u16.to_le_bytes());
        tiff.extend(1u32.to_le_bytes());
        tiff.extend(value.to_le_bytes());
        tiff.extend([0, 0]);
        tiff.extend(0u32.to_le_bytes());
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(tiff);
        let mut out = jpeg[..2].to_vec();
        out.extend([0xFF, 0xE1]);
        out.extend(((app1.len() + 2) as u16).to_be_bytes());
        out.extend(app1);
        out.extend(&jpeg[2..]);
        out
    }

    #[test]
    fn applies_exif_orientation() {
        // 16x8, left half black, right half white.
        let img = RgbaImage::from_fn(16, 8, |x, _| {
            let v = if x < 8 { 0 } else { 255 };
            image::Rgba([v, v, v, 255])
        });
        let dir = std::env::temp_dir().join(format!("bic-load-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rotated.jpg");
        // Orientation 6: rotate 90° clockwise to display.
        fs::write(&path, jpeg_with_orientation(&img, 6)).unwrap();
        let loaded = load(&path).unwrap().image;
        fs::remove_dir_all(&dir).ok();
        assert_eq!(loaded.dimensions(), (8, 16));
        // The black half (left) ends up on top after a clockwise turn.
        assert!(loaded.get_pixel(4, 2)[0] < 40);
        assert!(loaded.get_pixel(4, 13)[0] > 215);
    }
}
