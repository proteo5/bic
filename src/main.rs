mod bench;

use anyhow::{ensure, Context, Result};
use bic::{decode_rgba_with_limits, encode_rgba, is_png_fallback, DecodeLimits, EncodeOptions};
use clap::{Parser, Subcommand};
use heic_rs::{DecodeOptions, PixelLayout};
use minifb::{Key, Window, WindowOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

#[derive(Parser)]
#[command(about = "Experimental BIC1 planar RGBA codec and viewer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Decoder working-memory limit in MiB (decode, view, verify)
    #[arg(long, global = true, default_value_t = 512)]
    max_alloc_mib: usize,
}

#[derive(Subcommand)]
enum Command {
    Encode {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = 1)]
        quant_step: u8,
        #[arg(long, default_value_t = 0)]
        smooth_tolerance: u8,
        #[arg(long)]
        residual: bool,
        #[arg(long)]
        png_fallback: bool,
        #[arg(long, default_value_t = 9)]
        zstd_level: i32,
    },
    Decode {
        input: PathBuf,
        output: PathBuf,
    },
    View {
        input: PathBuf,
    },
    Verify {
        input: PathBuf,
        bic: PathBuf,
    },
    /// Compare lossless BIC with PNG, oxipng, WebP and QOI on images or folders
    Compare {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[arg(long, default_value_t = 9)]
        zstd_level: i32,
        /// Also write per-image results to this CSV file
        #[arg(long)]
        csv: Option<PathBuf>,
    },
    InstallAssociation,
}

pub(crate) fn load(path: &Path) -> Result<image::RgbaImage> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext == "heic" || ext == "heif" {
        let options = DecodeOptions::default()
            .with_layout(PixelLayout::Rgba8)
            .with_alpha(true);
        let decoded = heic_rs::decode(&data, &options).context("decoding HEIC/HEIF")?;
        return image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.data)
            .context("invalid HEIC RGBA dimensions");
    }
    Ok(image::load_from_memory(&data)?.to_rgba8())
}

fn read_bic(path: &Path, limits: DecodeLimits) -> Result<(u32, u32, Vec<u8>)> {
    let file = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    decode_rgba_with_limits(&file, limits).with_context(|| format!("decoding {}", path.display()))
}

fn view(path: &Path, limits: DecodeLimits) -> Result<()> {
    let (title, image) = if path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("bic"))
    {
        let (w, h, p) = read_bic(path, limits)?;
        (
            format!("BIC viewer — {}", path.display()),
            image::RgbaImage::from_raw(w, h, p).context("invalid BIC image")?,
        )
    } else {
        (format!("BIC viewer — {}", path.display()), load(path)?)
    };
    let scale = (1600.0 / image.width() as f32)
        .min(1000.0 / image.height() as f32)
        .min(1.0);
    let w = (image.width() as f32 * scale).round().max(1.0) as u32;
    let h = (image.height() as f32 * scale).round().max(1.0) as u32;
    let displayed = if (w, h) == (image.width(), image.height()) {
        image
    } else {
        image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle)
    };
    let pixels: Vec<u32> = displayed
        .pixels()
        .map(|p| {
            let a = p[3] as u32;
            let bg = 36;
            let r = (p[0] as u32 * a + bg * (255 - a)) / 255;
            let g = (p[1] as u32 * a + bg * (255 - a)) / 255;
            let b = (p[2] as u32 * a + bg * (255 - a)) / 255;
            (r << 16) | (g << 8) | b
        })
        .collect();
    let mut window = Window::new(
        &title,
        w as usize,
        h as usize,
        WindowOptions {
            resize: true,
            ..Default::default()
        },
    )?;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        window.update_with_buffer(&pixels, w as usize, h as usize)?;
    }
    Ok(())
}

fn install_association() -> Result<()> {
    let exe = std::env::current_exe()?;
    let open = format!("\"{}\" view \"%1\"", exe.display());
    for (key, value) in [
        (r"HKCU\Software\Classes\.bic", "BIC.Image"),
        (r"HKCU\Software\Classes\BIC.Image", "BIC image"),
        (r"HKCU\Software\Classes\BIC.Image\shell\open\command", &open),
    ] {
        let status = ProcessCommand::new("reg")
            .args(["add", key, "/ve", "/d", value, "/f"])
            .status()?;
        ensure!(status.success(), "could not register {key}");
    }
    println!(
        ".bic is associated with {} for this Windows user",
        exe.display()
    );
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let limits = DecodeLimits {
        max_alloc: cli.max_alloc_mib.saturating_mul(1 << 20),
        ..Default::default()
    };
    match cli.command {
        Command::Encode {
            input,
            output,
            quant_step,
            smooth_tolerance,
            residual,
            png_fallback,
            zstd_level,
        } => {
            let i = load(&input)?;
            let f = encode_rgba(
                i.width(),
                i.height(),
                i.as_raw(),
                EncodeOptions {
                    quant_step,
                    smooth_tolerance,
                    residual,
                    png_fallback,
                    zstd_level,
                },
            )?;
            fs::write(&output, &f)?;
            println!(
                "wrote {} bytes ({})",
                f.len(),
                if is_png_fallback(&f)? {
                    "PNG fallback"
                } else {
                    "native BIC"
                }
            );
        }
        Command::Decode { input, output } => {
            let (w, h, p) = read_bic(&input, limits)?;
            image::RgbaImage::from_raw(w, h, p)
                .context("decoded size mismatch")?
                .save(&output)
                .with_context(|| format!("writing {}", output.display()))?;
        }
        Command::View { input } => view(&input, limits)?,
        Command::Verify { input, bic } => {
            let original = load(&input)?;
            let (w, h, p) = read_bic(&bic, limits)?;
            ensure!(
                w == original.width() && h == original.height() && p == *original.as_raw(),
                "roundtrip pixels differ"
            );
            println!("exact pixel roundtrip verified");
        }
        Command::Compare {
            inputs,
            zstd_level,
            csv,
        } => bench::run(&inputs, zstd_level, csv.as_deref())?,
        Command::InstallAssociation => install_association()?,
    };
    Ok(())
}
