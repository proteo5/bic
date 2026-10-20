//! Corpus benchmark: lossless BIC against the common lossless formats.

use anyhow::{anyhow, ensure, Result};
use bic::{decode_rgba, encode_rgba, EncodeOptions};
use image::{codecs::webp::WebPEncoder, ExtendedColorType, ImageFormat, RgbaImage};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const EXTENSIONS: [&str; 11] = [
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "qoi", "heic", "heif",
];
const OXIPNG_PRESET: u8 = 2;

type SizeOf = fn(&Row) -> usize;

struct Row {
    name: String,
    width: u32,
    height: u32,
    png: usize,
    oxipng: usize,
    webp: usize,
    qoi: usize,
    bic: usize,
    encode: Duration,
    decode: Duration,
}

pub fn run(paths: &[PathBuf], zstd_level: i32, csv: Option<&Path>) -> Result<()> {
    let files = collect(paths)?;
    ensure!(!files.is_empty(), "no supported images found");
    if cfg!(debug_assertions) {
        eprintln!("note: debug build; timings are not representative (use --release)");
    }
    let mut rows = Vec::new();
    let mut failed = 0;
    for file in &files {
        match measure(file, zstd_level) {
            Ok(row) => rows.push(row),
            Err(e) => {
                eprintln!("failed {}: {e:#}", file.display());
                failed += 1;
            }
        }
    }
    ensure!(!rows.is_empty(), "no image could be measured");
    print_table(&rows);
    print_summary(&rows);
    if let Some(csv) = csv {
        write_csv(csv, &rows)?;
        println!("\nwrote {}", csv.display());
    }
    // A run with failures (unreadable input or a broken roundtrip) is not a
    // valid measurement, so it must not look like a success.
    ensure!(failed == 0, "{failed} of {} images failed", files.len());
    Ok(())
}

/// Expands directories (recursively) into supported image files, sorted.
fn collect(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut children: Vec<PathBuf> = fs::read_dir(path)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<_>>()?;
            children.retain(|p| p.is_dir() || is_supported(p));
            out.extend(collect(&children)?);
        } else {
            ensure!(path.is_file(), "{} does not exist", path.display());
            out.push(path.clone());
        }
    }
    out.sort();
    Ok(out)
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| EXTENSIONS.contains(&x.to_ascii_lowercase().as_str()))
}

fn measure(path: &Path, zstd_level: i32) -> Result<Row> {
    let image = crate::load(path)?;
    let (width, height) = image.dimensions();
    let png = encode_as(&image, ImageFormat::Png)?;
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
    let qoi = encode_as(&image, ImageFormat::Qoi)?.len();

    let options = EncodeOptions {
        zstd_level,
        ..Default::default()
    };
    let t = Instant::now();
    let bic = encode_rgba(width, height, image.as_raw(), options)?;
    let encode = t.elapsed();
    let t = Instant::now();
    let (_, _, decoded) = decode_rgba(&bic)?;
    let decode = t.elapsed();
    ensure!(decoded == *image.as_raw(), "BIC roundtrip differs");

    Ok(Row {
        name: path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        ),
        width,
        height,
        png: png.len(),
        oxipng,
        webp: webp.len(),
        qoi,
        bic: bic.len(),
        encode,
        decode,
    })
}

fn encode_as(image: &RgbaImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, format)?;
    Ok(out.into_inner())
}

fn kib(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / 1024.0)
}

fn print_table(rows: &[Row]) {
    println!(
        "{:<28} {:>11} {:>9} {:>9} {:>9} {:>9} {:>9} {:>8} {:>9} {:>9}",
        "image",
        "size",
        "PNG KiB",
        "oxi KiB",
        "WebP KiB",
        "QOI KiB",
        "BIC KiB",
        "BIC/oxi",
        "enc ms",
        "dec ms"
    );
    for r in rows {
        let name: String = if r.name.chars().count() > 28 {
            r.name.chars().take(27).chain(['…']).collect()
        } else {
            r.name.clone()
        };
        println!(
            "{:<28} {:>11} {:>9} {:>9} {:>9} {:>9} {:>9} {:>8.3} {:>9.1} {:>9.1}",
            name,
            format!("{}x{}", r.width, r.height),
            kib(r.png),
            kib(r.oxipng),
            kib(r.webp),
            kib(r.qoi),
            kib(r.bic),
            r.bic as f64 / r.oxipng as f64,
            r.encode.as_secs_f64() * 1e3,
            r.decode.as_secs_f64() * 1e3,
        );
    }
}

fn print_summary(rows: &[Row]) {
    let total = |f: fn(&Row) -> usize| rows.iter().map(f).sum::<usize>();
    let bic = total(|r| r.bic);
    println!("\n{} images, BIC total {} KiB", rows.len(), kib(bic));
    println!(
        "{:<26} {:>10} {:>12} {:>12} {:>10}",
        "baseline", "total KiB", "BIC/total", "BIC/geomean", "BIC wins"
    );
    let baselines: [(&str, SizeOf); 4] = [
        ("PNG (image-rs)", |r| r.png),
        ("oxipng preset 2", |r| r.oxipng),
        ("WebP lossless (image-rs)", |r| r.webp),
        ("QOI", |r| r.qoi),
    ];
    for (name, size) in baselines {
        let log_sum: f64 = rows
            .iter()
            .map(|r| (r.bic as f64 / size(r) as f64).ln())
            .sum();
        let wins = rows.iter().filter(|r| r.bic < size(r)).count();
        println!(
            "{:<26} {:>10} {:>12.3} {:>12.3} {:>7}/{}",
            name,
            kib(total(size)),
            bic as f64 / total(size) as f64,
            (log_sum / rows.len() as f64).exp(),
            wins,
            rows.len()
        );
    }
    println!("\nRatios below 1.000 mean BIC is smaller. Results depend on the corpus.");
}

fn write_csv(path: &Path, rows: &[Row]) -> Result<()> {
    let mut out = String::from(
        "image,width,height,png,oxipng,webp_lossless,qoi,bic,bic_encode_ms,bic_decode_ms\n",
    );
    for r in rows {
        out += &format!(
            "\"{}\",{},{},{},{},{},{},{},{:.3},{:.3}\n",
            r.name.replace('"', "\"\""),
            r.width,
            r.height,
            r.png,
            r.oxipng,
            r.webp,
            r.qoi,
            r.bic,
            r.encode.as_secs_f64() * 1e3,
            r.decode.as_secs_f64() * 1e3,
        );
    }
    fs::write(path, out)?;
    Ok(())
}
