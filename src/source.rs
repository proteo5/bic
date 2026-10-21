//! Reading encoded source images into upright 8-bit RGBA.
//!
//! PNG, JPEG, WebP and HEIC/HEIF are always readable; with the `cli` feature
//! BMP, GIF, TIFF and QOI are too. Only the always-readable formats can be
//! embedded unchanged in a BIC file, so every decoder can open the result.

use crate::{checked_pixels, DecodeLimits, Metadata};
use anyhow::{bail, Context, Result};
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::io::Cursor;

/// Formats a BIC file may store unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbeddableFormat {
    /// Portable Network Graphics.
    Png,
    /// JPEG (JFIF or EXIF).
    Jpeg,
    /// WebP, lossy or lossless.
    WebP,
    /// HEIF, including HEIC.
    Heif,
}

/// A decoded source image.
#[derive(Clone, Debug)]
pub struct Source {
    /// Width in pixels, after orientation.
    pub width: u32,
    /// Height in pixels, after orientation.
    pub height: u32,
    /// Upright (EXIF/HEIF orientation applied) 8-bit RGBA, row-major.
    pub rgba: Vec<u8>,
    /// Metadata read from the file, such as its ICC profile.
    pub metadata: Metadata,
    /// Set when the file could be stored unchanged in a BIC file.
    pub embeddable: Option<EmbeddableFormat>,
    /// Conversions a caller may want to report, such as bit-depth loss.
    pub notes: Vec<String>,
}

/// Identifies an embeddable format from the file's signature.
pub fn embeddable_format(bytes: &[u8]) -> Option<EmbeddableFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(EmbeddableFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(EmbeddableFormat::Jpeg)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(EmbeddableFormat::WebP)
    } else if is_heif(bytes) {
        Some(EmbeddableFormat::Heif)
    } else {
        None
    }
}

/// ISO-BMFF `ftyp` box with a HEIF/HEIC major brand.
fn is_heif(bytes: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 9] = [
        b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"heif",
    ];
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && BRANDS.iter().any(|b| &bytes[8..12] == *b)
}

/// Decodes an image file. Sizes are checked against `limits` before the
/// pixels are decoded.
pub fn read_source(bytes: &[u8], limits: DecodeLimits) -> Result<Source> {
    let embeddable = embeddable_format(bytes);
    let mut source = if embeddable == Some(EmbeddableFormat::Heif) {
        #[cfg(feature = "formats")]
        {
            read_heif(bytes, limits)?
        }
        #[cfg(not(feature = "formats"))]
        bail!("HEIF sources need the `formats` feature")
    } else {
        read_with_image(bytes, limits)?
    };
    source.embeddable = embeddable;
    Ok(source)
}

fn read_with_image(bytes: &[u8], limits: DecodeLimits) -> Result<Source> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut image_limits = image::Limits::default();
    image_limits.max_alloc = Some(limits.max_alloc as u64);
    reader.limits(image_limits);
    let mut decoder = reader
        .into_decoder()
        .context("unsupported or corrupt image")?;
    let (w, h) = decoder.dimensions();
    checked_pixels(w, h, limits.max_pixels)?;
    // Profiles and orientation are best effort: a broken tag should not make
    // an otherwise readable image fail.
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
    let rgba = image.to_rgba8();
    Ok(Source {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
        metadata: Metadata { icc_profile },
        embeddable: None,
        notes,
    })
}

#[cfg(feature = "formats")]
fn read_heif(bytes: &[u8], limits: DecodeLimits) -> Result<Source> {
    let options = heic_rs::DecodeOptions::default()
        .with_layout(heic_rs::PixelLayout::Rgba8)
        .with_alpha(true)
        .with_max_pixels(Some(limits.max_pixels as u64));
    // heic-rs applies the HEIF rotation and mirror properties itself.
    let decoded = heic_rs::decode(bytes, &options).context("decoding HEIC/HEIF")?;
    checked_pixels(decoded.width, decoded.height, limits.max_pixels)?;
    if decoded.data.len() != decoded.width as usize * decoded.height as usize * 4 {
        bail!("invalid HEIC RGBA dimensions");
    }
    let icc_profile = heic_rs::context::Context::open(bytes)
        .ok()
        .and_then(|ctx| ctx.props(ctx.meta.primary).ok())
        .and_then(|props| props.icc.map(<[u8]>::to_vec));
    let mut notes = Vec::new();
    if heic_rs::probe(bytes).is_ok_and(|info| info.bit_depth > 8) {
        notes.push("samples above 8 bits were reduced to 8 bits".into());
    }
    Ok(Source {
        width: decoded.width,
        height: decoded.height,
        rgba: decoded.data,
        metadata: Metadata { icc_profile },
        embeddable: None,
        notes,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::RgbaImage;

    /// A JPEG of `img` with an EXIF APP1 segment whose Orientation is `value`.
    pub(crate) fn jpeg_with_orientation(img: &RgbaImage, value: u16) -> Vec<u8> {
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

    #[cfg(feature = "formats")]
    #[test]
    fn applies_exif_orientation() {
        // 16x8, left half black, right half white.
        let img = RgbaImage::from_fn(16, 8, |x, _| {
            let v = if x < 8 { 0 } else { 255 };
            image::Rgba([v, v, v, 255])
        });
        // Orientation 6: rotate 90° clockwise to display.
        let source = read_source(&jpeg_with_orientation(&img, 6), DecodeLimits::default()).unwrap();
        let loaded = RgbaImage::from_raw(source.width, source.height, source.rgba).unwrap();
        assert_eq!(loaded.dimensions(), (8, 16));
        // The black half (left) ends up on top after a clockwise turn.
        assert!(loaded.get_pixel(4, 2)[0] < 40);
        assert!(loaded.get_pixel(4, 13)[0] > 215);
        assert_eq!(source.embeddable, Some(EmbeddableFormat::Jpeg));
    }

    #[test]
    fn identifies_embeddable_formats_by_signature() {
        let png = {
            let mut out = Vec::new();
            RgbaImage::new(2, 2)
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .unwrap();
            out
        };
        assert_eq!(embeddable_format(&png), Some(EmbeddableFormat::Png));
        assert_eq!(
            embeddable_format(b"RIFF\0\0\0\0WEBPVP8L"),
            Some(EmbeddableFormat::WebP)
        );
        assert_eq!(
            embeddable_format(b"\0\0\0\x18ftypheic\0\0\0\0"),
            Some(EmbeddableFormat::Heif)
        );
        assert_eq!(embeddable_format(b"BM\0\0\0\0\0\0"), None);
        assert_eq!(embeddable_format(b"BIC2"), None);
    }

    #[test]
    fn refuses_images_over_the_pixel_limit_before_decoding() {
        let img = RgbaImage::new(64, 64);
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let limits = DecodeLimits {
            max_pixels: 1000,
            ..Default::default()
        };
        assert!(read_source(&png, limits).is_err());
    }
}
