mod bench;
mod inspect;
mod load;
mod viewer;

use anyhow::{bail, ensure, Context, Result};
use bic::{
    decode_with_metadata, encode_source, storage, DecodeLimits, Decoded, EncodeOptions, Storage,
};
use clap::{Parser, Subcommand};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

#[derive(Parser)]
#[command(about = "Experimental BIC2 planar RGBA codec and viewer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Decoder working-memory limit in MiB (decode, view, verify)
    #[arg(long, global = true, default_value_t = 512)]
    max_alloc_mib: usize,
}

#[derive(Subcommand)]
enum Command {
    /// Encode an image to BIC
    Encode {
        input: PathBuf,
        output: PathBuf,
        /// Largest allowed per-sample error; 0 is lossless
        #[arg(long, default_value_t = 0)]
        max_error: u8,
        /// Store a PNG instead when it is smaller (lossless only)
        #[arg(long)]
        png_fallback: bool,
        /// Only use predictors that decode fastest (larger files on photos)
        #[arg(long)]
        fast_decode: bool,
        /// Always store BIC's own encoding, even when the original file is
        /// smaller (by default a smaller PNG, JPEG, WebP or HEIF is kept as is)
        #[arg(long)]
        transcode: bool,
        #[arg(long, default_value_t = 19)]
        zstd_level: i32,
    },
    /// Decode a BIC file; the output format follows the extension
    Decode { input: PathBuf, output: PathBuf },
    /// Show a BIC file or any supported image in a window (Esc closes it)
    View { input: PathBuf },
    /// Check a BIC file against its original image
    Verify {
        input: PathBuf,
        bic: PathBuf,
        /// Accept differences up to this value (for near-lossless files)
        #[arg(long, default_value_t = 0)]
        max_error: u8,
    },
    /// Compare BIC with PNG, oxipng, WebP and QOI on images or folders
    Compare {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Largest allowed per-sample error; reports PSNR when above 0
        #[arg(long, default_value_t = 0)]
        max_error: u8,
        /// Only use predictors that decode fastest (larger files on photos)
        #[arg(long)]
        fast_decode: bool,
        #[arg(long, default_value_t = 19)]
        zstd_level: i32,
        /// Also write per-image results to this CSV file
        #[arg(long)]
        csv: Option<PathBuf>,
    },
    /// Measure an image: sizes of the lossless alternatives and of BIC. Used by
    /// build tools to decide whether to store an image as BIC.
    Inspect {
        input: PathBuf,
        /// Print JSON for scripts instead of a summary
        #[arg(long)]
        json: bool,
    },
    /// Open .bic files with bic-view for the current Windows user
    InstallAssociation,
}

fn read_bic(path: &Path, limits: DecodeLimits) -> Result<Decoded> {
    let file = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    decode_with_metadata(&file, limits).with_context(|| format!("decoding {}", path.display()))
}

/// Writes a decoded image. PNG output keeps the ICC profile; other formats
/// are written by the `image` crate without it.
fn write_image(path: &Path, decoded: Decoded) -> Result<()> {
    let is_png = path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("png"));
    if is_png {
        let file =
            fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut encoder = PngEncoder::new(std::io::BufWriter::new(file));
        if let Some(icc) = decoded.metadata.icc_profile {
            encoder.set_icc_profile(icc)?;
        }
        encoder.write_image(
            &decoded.rgba,
            decoded.width,
            decoded.height,
            ExtendedColorType::Rgba8,
        )?;
        return Ok(());
    }
    if decoded.metadata.icc_profile.is_some() {
        eprintln!("note: the ICC profile is only written to PNG output");
    }
    let image = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba)
        .context("decoded size mismatch")?;
    let no_alpha = path.extension().is_some_and(|x| {
        ["jpg", "jpeg"]
            .iter()
            .any(|ext| x.eq_ignore_ascii_case(ext))
    });
    let result = if no_alpha {
        // JPEG has no alpha channel.
        if image.pixels().any(|p| p[3] != 255) {
            eprintln!("note: transparency is dropped in JPEG output");
        }
        image::DynamicImage::ImageRgba8(image).to_rgb8().save(path)
    } else {
        image.save(path)
    };
    result.with_context(|| format!("writing {}", path.display()))
}

fn install_association() -> Result<()> {
    let exe = std::env::current_exe()?;
    // Prefer the console-less viewer next to this executable, so double-
    // clicking a file does not open a terminal window.
    let viewer = exe.with_file_name(format!("bic-view{}", std::env::consts::EXE_SUFFIX));
    let open = if viewer.is_file() {
        format!("\"{}\" \"%1\"", viewer.display())
    } else {
        eprintln!(
            "note: {} not found; using `bic view`, which opens a console window",
            viewer.display()
        );
        format!("\"{}\" view \"%1\"", exe.display())
    };
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
    println!(".bic files now open with: {open}");
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
            max_error,
            png_fallback,
            fast_decode,
            transcode,
            zstd_level,
        } => {
            let loaded = load::load(&input)?;
            for note in &loaded.source.notes {
                eprintln!("note: {note}");
            }
            let mut source = loaded.source;
            source.rgba = loaded.image.into_raw();
            let options = EncodeOptions {
                max_error,
                png_fallback,
                fast_decode,
                keep_original: !transcode,
                zstd_level,
            };
            let f = encode_source(&source, &loaded.original, options)?;
            fs::write(&output, &f)?;
            let how = match storage(&f)? {
                Storage::Native if source.metadata.icc_profile.is_some() => {
                    "native BIC, with ICC profile"
                }
                Storage::Native => "native BIC",
                Storage::PngFallback => "PNG fallback",
                Storage::Original => "original file kept: smaller than BIC's own encoding",
            };
            println!(
                "wrote {} bytes ({how}); original {} bytes",
                f.len(),
                loaded.original.len()
            );
        }
        Command::Decode { input, output } => write_image(&output, read_bic(&input, limits)?)?,
        Command::View { input } => viewer::view(Some(&input), limits)?,
        Command::Verify {
            input,
            bic,
            max_error,
        } => {
            let original = load::load(&input)?.image;
            let d = read_bic(&bic, limits)?;
            ensure!(
                (d.width, d.height) == original.dimensions(),
                "dimensions differ: {}x{} vs {}x{}",
                d.width,
                d.height,
                original.width(),
                original.height()
            );
            let worst = d
                .rgba
                .iter()
                .zip(original.as_raw())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            match worst {
                0 => println!("exact pixel roundtrip verified"),
                n if n <= max_error => println!("verified: samples differ by at most {n}"),
                n => bail!("samples differ by up to {n} (allowed: {max_error})"),
            }
        }
        Command::Compare {
            inputs,
            max_error,
            fast_decode,
            zstd_level,
            csv,
        } => {
            let options = EncodeOptions {
                max_error,
                fast_decode,
                zstd_level,
                ..Default::default()
            };
            bench::run(&inputs, options, csv.as_deref())?
        }
        Command::Inspect { input, json } => {
            let report = inspect::inspect(&input)?;
            if json {
                println!("{}", report.to_json());
            } else {
                println!(
                    "{}x{}, {} colors
PNG {} B, oxipng {} B, WebP lossless {} B
BIC {} B ({:?})",
                    report.width,
                    report.height,
                    report.distinct_colors,
                    report.png,
                    report.oxipng,
                    report.webp_lossless,
                    report.bic,
                    report.bic_storage
                );
            }
        }
        Command::InstallAssociation => install_association()?,
    };
    Ok(())
}
