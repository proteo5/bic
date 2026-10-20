//! Writes a deterministic synthetic benchmark corpus of PNG images.
//!
//! cargo run --release --example gen_corpus -- <output-dir>
//!
//! The images cover what BIC targets (flat UI, diagrams, charts, pixel art,
//! icons with alpha) plus two controls with no flat regions: a smooth radial
//! gradient and pure noise. The same output is produced on every run.

use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use std::{fs, path::PathBuf};

const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
const INK: Rgba<u8> = Rgba([33, 37, 41, 255]);
const ACCENT: Rgba<u8> = Rgba([13, 110, 253, 255]);
const MUTED: Rgba<u8> = Rgba([233, 236, 239, 255]);
const BORDER: Rgba<u8> = Rgba([173, 181, 189, 255]);

/// Small LCG so the corpus never depends on an external RNG's stability.
/// The raw state's low bits repeat with short periods, which a predictor can
/// exploit, so the output goes through an integer hash finalizer.
struct Lcg(u32);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut x = self.0;
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^ (x >> 16)
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

fn rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, c: Rgba<u8>) {
    for yy in y..(y + h).min(img.height()) {
        for xx in x..(x + w).min(img.width()) {
            img.put_pixel(xx, yy, c);
        }
    }
}

fn outline(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, c: Rgba<u8>) {
    rect(img, x, y, w, 1, c);
    rect(img, x, y + h - 1, w, 1, c);
    rect(img, x, y, 1, h, c);
    rect(img, x + w - 1, y, 1, h, c);
}

/// Rows of glyph-like marks, standing in for rendered text.
fn text(img: &mut RgbaImage, rng: &mut Lcg, x: u32, y: u32, width: u32, lines: u32, c: Rgba<u8>) {
    for line in 0..lines {
        let mut cx = x;
        let end = x + width - rng.below(width / 3);
        while cx < end {
            let word = 3 + rng.below(6);
            for _ in 0..word {
                let gw = 4 + rng.below(3);
                for gy in 0..9 {
                    for gx in 0..gw {
                        if rng.below(3) == 0 {
                            rect(img, cx + gx, y + line * 16 + gy, 1, 1, c);
                        }
                    }
                }
                cx += gw + 1;
            }
            cx += 5;
        }
    }
}

fn ui_mock() -> RgbaImage {
    let mut rng = Lcg(1);
    let mut img = RgbaImage::from_pixel(1280, 800, WHITE);
    rect(&mut img, 0, 0, 1280, 56, INK);
    rect(&mut img, 0, 56, 240, 744, MUTED);
    for i in 0..8 {
        text(&mut img, &mut rng, 24, 88 + i * 40, 160, 1, INK);
    }
    rect(&mut img, 0, 128, 240, 32, ACCENT);
    for (i, row) in (0..3).flat_map(|r| (0..3).map(move |c| (r, c))).enumerate() {
        let (x, y) = (272 + row.1 * 328, 88 + row.0 * 232);
        outline(&mut img, x, y, 312, 216, BORDER);
        text(&mut img, &mut rng, x + 16, y + 16, 260, 5, INK);
        let button = if i % 2 == 0 { ACCENT } else { INK };
        rect(&mut img, x + 16, y + 160, 120, 36, button);
    }
    img
}

fn diagram() -> RgbaImage {
    let mut rng = Lcg(2);
    let mut img = RgbaImage::from_pixel(1024, 768, WHITE);
    let boxes = [
        (64, 64),
        (416, 64),
        (768, 64),
        (240, 352),
        (592, 352),
        (416, 608),
    ];
    for (i, &(x, y)) in boxes.iter().enumerate() {
        let fill = [
            MUTED,
            Rgba([209, 231, 221, 255]),
            Rgba([255, 243, 205, 255]),
        ][i % 3];
        rect(&mut img, x, y, 192, 96, fill);
        outline(&mut img, x, y, 192, 96, INK);
        text(&mut img, &mut rng, x + 16, y + 24, 160, 3, INK);
    }
    for &(x, y) in &boxes[..5] {
        rect(&mut img, x + 95, y + 96, 2, 608 - y - 96, INK);
    }
    img
}

fn chart() -> RgbaImage {
    let mut img = RgbaImage::from_pixel(800, 600, WHITE);
    for y in (80..520).step_by(55) {
        rect(&mut img, 80, y, 680, 1, MUTED);
    }
    rect(&mut img, 80, 520, 680, 2, INK);
    rect(&mut img, 80, 80, 2, 442, INK);
    let heights = [180, 320, 260, 410, 150, 370, 290, 220];
    for (i, h) in heights.into_iter().enumerate() {
        let c = [ACCENT, Rgba([25, 135, 84, 255])][i % 2];
        rect(&mut img, 110 + i as u32 * 80, 520 - h, 48, h, c);
    }
    img
}

fn pixel_art() -> RgbaImage {
    let mut rng = Lcg(3);
    let palette = [
        Rgba([20, 12, 28, 255]),
        Rgba([68, 36, 52, 255]),
        Rgba([48, 52, 109, 255]),
        Rgba([78, 74, 78, 255]),
        Rgba([133, 76, 48, 255]),
        Rgba([52, 101, 36, 255]),
        Rgba([208, 70, 72, 255]),
        Rgba([109, 170, 44, 255]),
    ];
    let mut img = RgbaImage::new(320, 240);
    for ty in 0..30 {
        for tx in 0..40 {
            let c = palette[rng.below(8) as usize];
            rect(&mut img, tx * 8, ty * 8, 8, 8, c);
        }
    }
    img
}

fn icon_alpha() -> RgbaImage {
    let mut img = RgbaImage::new(256, 256);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let d = ((x as f32 - 127.5).powi(2) + (y as f32 - 127.5).powi(2)).sqrt();
        let coverage = (100.0 - d).clamp(0.0, 1.0);
        let ring = d > 70.0 && d < 84.0;
        let c = if ring { WHITE } else { ACCENT };
        *p = Rgba([c[0], c[1], c[2], (coverage * 255.0).round() as u8]);
    }
    img
}

fn gradient() -> RgbaImage {
    // Radial, so no row or column repeats exactly.
    RgbaImage::from_fn(512, 512, |x, y| {
        let d = ((x as f32 - 160.0).powi(2) + (y as f32 - 352.0).powi(2)).sqrt();
        let v = (d * 255.0 / 520.0) as u8;
        Rgba([v, 255 - v, ((x + y) / 4) as u8, 255])
    })
}

fn noise() -> RgbaImage {
    let mut rng = Lcg(4);
    RgbaImage::from_fn(256, 256, |_, _| {
        let v = rng.next();
        Rgba([v as u8, (v >> 8) as u8, (v >> 16) as u8, 255])
    })
}

type Generator = fn() -> RgbaImage;

fn main() -> Result<()> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("usage: gen_corpus <output-dir>")?,
    );
    fs::create_dir_all(&dir)?;
    let images: [(&str, Generator); 7] = [
        ("ui_mock", ui_mock),
        ("diagram", diagram),
        ("chart", chart),
        ("pixel_art", pixel_art),
        ("icon_alpha", icon_alpha),
        ("control_gradient", gradient),
        ("control_noise", noise),
    ];
    for (name, make) in images {
        let path = dir.join(format!("{name}.png"));
        make().save(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
