//! Native image viewer. Shared by `bic view` and the console-less `bic-view`
//! binary used for the Windows file association.
//!
//! It shows a `.bic` file or any image `load` reads, opens other images with
//! File > Open (Ctrl+O) and saves the current one as BIC with File > Save as
//! BIC (Ctrl+S). Saving runs in the background because the encoder trades
//! time for file size.

use crate::load::{self, EXTENSIONS};
use anyhow::{Context, Result};
use bic::{decode_with_metadata, encode_rgba_with_metadata, DecodeLimits, EncodeOptions, Metadata};
use image::{imageops::FilterType, RgbaImage};
use minifb::{Key, KeyRepeat, Menu, ScaleMode, Window, WindowOptions, MENU_KEY_CTRL};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

const MAX_WINDOW: (f32, f32) = (1600.0, 1000.0);
const FPS: usize = 30;
const CHECKER: usize = 8;
const CHECKER_SHADES: [u32; 2] = [0x2c, 0x3c];
/// Window area around the image.
const BACKGROUND: u32 = 0x1e1e1e;
const MENU_OPEN: usize = 1;
const MENU_SAVE: usize = 2;

/// The image on screen: full resolution for saving and for large windows,
/// plus a high-quality preview that fits the screen, from which smaller
/// window sizes are rendered quickly.
struct Shown {
    path: PathBuf,
    image: RgbaImage,
    preview: RgbaImage,
    metadata: Metadata,
}

/// What was last drawn, at exactly the window's size, so minifb never has to
/// scale it (its scaling skips pixels and makes photos look broken).
struct Frame {
    buffer: Vec<u32>,
    size: (usize, usize),
}

/// A save running on a background thread.
struct Saving {
    target: PathBuf,
    done: mpsc::Receiver<Result<usize>>,
}

/// Reads a `.bic` file or any image `load` reads, with its metadata.
pub fn open(path: &Path, limits: DecodeLimits) -> Result<(RgbaImage, Metadata)> {
    if is_bic(path) {
        let file = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let d = decode_with_metadata(&file, limits)
            .with_context(|| format!("decoding {}", path.display()))?;
        let image =
            RgbaImage::from_raw(d.width, d.height, d.rgba).context("decoded size mismatch")?;
        Ok((image, d.metadata))
    } else {
        let loaded = load::load(path)?;
        let metadata = Metadata {
            icc_profile: loaded.icc_profile,
        };
        Ok((loaded.image, metadata))
    }
}

/// Shows `path`, or asks for an image first when there is none. Returns when
/// the window is closed or Esc is pressed.
pub fn view(path: Option<&Path>, limits: DecodeLimits) -> Result<()> {
    let first = match path {
        Some(p) => p.to_path_buf(),
        None => match pick_image(None) {
            Some(p) => p,
            None => return Ok(()),
        },
    };
    let mut current = show(&first, limits)?;
    let (pw, ph) = current.preview.dimensions();
    let mut window = Window::new(
        &title(&current, None),
        pw as usize,
        ph as usize,
        WindowOptions {
            resize: true,
            // Frames are rendered at the window's size; this only matters
            // for the moment between a resize and the next frame.
            scale_mode: ScaleMode::Center,
            ..Default::default()
        },
    )?;
    let mut frame = render_frame(&current, window.get_size());
    // The image only changes on user action, so a low frame rate keeps the
    // CPU idle.
    window.set_target_fps(FPS);
    let mut menu = Menu::new("File")?;
    menu.add_item("Open…", MENU_OPEN)
        .shortcut(Key::O, MENU_KEY_CTRL)
        .build();
    menu.add_item("Save as BIC…", MENU_SAVE)
        .shortcut(Key::S, MENU_KEY_CTRL)
        .build();
    window.add_menu(&menu);

    let mut saving: Option<Saving> = None;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Where the platform has no native menus, the shortcuts still work.
        let ctrl = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
        let action = window.is_menu_pressed().or_else(|| {
            if ctrl && window.is_key_pressed(Key::O, KeyRepeat::No) {
                Some(MENU_OPEN)
            } else if ctrl && window.is_key_pressed(Key::S, KeyRepeat::No) {
                Some(MENU_SAVE)
            } else {
                None
            }
        });
        match action {
            Some(MENU_OPEN) => {
                if let Some(path) = pick_image(Some(&current.path)) {
                    match show(&path, limits) {
                        Ok(shown) => {
                            current = shown;
                            frame = render_frame(&current, window.get_size());
                            window.set_title(&title(&current, None));
                        }
                        Err(e) => {
                            error_dialog(&format!("Could not open {}:\n\n{e:#}", path.display()))
                        }
                    }
                }
            }
            Some(MENU_SAVE) if saving.is_some() => {}
            Some(MENU_SAVE) => {
                if let Some(target) = pick_save_target(&current.path) {
                    let status = format!("saving {}…", file_name(&target));
                    window.set_title(&title(&current, Some(&status)));
                    saving = Some(start_save(&current, target));
                }
            }
            _ => {}
        }
        let finished = saving.as_ref().and_then(|s| s.done.try_recv().ok());
        if let Some(result) = finished {
            let target = saving.take().expect("a save was running").target;
            match result {
                Ok(bytes) => {
                    let status = format!(
                        "saved {} ({:.1} KiB)",
                        file_name(&target),
                        bytes as f64 / 1024.0
                    );
                    window.set_title(&title(&current, Some(&status)));
                }
                Err(e) => {
                    window.set_title(&title(&current, None));
                    error_dialog(&format!("Could not save {}:\n\n{e:#}", target.display()));
                }
            }
        }
        let size = window.get_size();
        if size != frame.size && size.0 > 0 && size.1 > 0 {
            frame = render_frame(&current, size);
        }
        window.update_with_buffer(&frame.buffer, frame.size.0, frame.size.1)?;
    }
    // Let a save in progress finish rather than abandon it on exit.
    if let Some(s) = saving {
        if let Ok(Err(e)) = s.done.recv() {
            error_dialog(&format!("Could not save {}:\n\n{e:#}", s.target.display()));
        }
    }
    Ok(())
}

/// Shows an error to someone who may have no console to read.
pub fn error_dialog(message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("BIC viewer")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn show(path: &Path, limits: DecodeLimits) -> Result<Shown> {
    let (image, metadata) = open(path, limits)?;
    let preview = fit(
        &image,
        MAX_WINDOW.0 as u32,
        MAX_WINDOW.1 as u32,
        FilterType::Lanczos3,
    );
    Ok(Shown {
        path: path.to_path_buf(),
        image,
        preview,
        metadata,
    })
}

/// Scales `image` down (never up) to fit `max_w` × `max_h`, keeping its
/// proportions. Both filters average source pixels, unlike nearest-neighbor
/// scaling, so photos stay smooth at any size.
fn fit(image: &RgbaImage, max_w: u32, max_h: u32, filter: FilterType) -> RgbaImage {
    let (w, h) = fitted_size(image.dimensions(), (max_w, max_h));
    if (w, h) == image.dimensions() {
        image.clone()
    } else {
        image::imageops::resize(image, w, h, filter)
    }
}

/// The largest size within `bounds` with the image's proportions, at most
/// the image's own size.
fn fitted_size((iw, ih): (u32, u32), (bw, bh): (u32, u32)) -> (u32, u32) {
    let scale = (bw as f64 / iw.max(1) as f64)
        .min(bh as f64 / ih.max(1) as f64)
        .min(1.0);
    let w = (iw as f64 * scale).round().max(1.0) as u32;
    let h = (ih as f64 * scale).round().max(1.0) as u32;
    (w, h)
}

/// Draws the image centered in a window of `size`, scaled to fit and with
/// transparency composed over a checkerboard. Sizes up to the preview are
/// rendered from the preview, which is fast; larger ones from the original,
/// which keeps detail.
fn render_frame(shown: &Shown, size: (usize, usize)) -> Frame {
    let (fw, fh) = (size.0.max(1), size.1.max(1));
    let (tw, th) = fitted_size(shown.image.dimensions(), (fw as u32, fh as u32));
    let source = if tw <= shown.preview.width() && th <= shown.preview.height() {
        &shown.preview
    } else {
        &shown.image
    };
    let scaled;
    let shown_image = if source.dimensions() == (tw, th) {
        source
    } else {
        scaled = image::imageops::resize(source, tw, th, FilterType::CatmullRom);
        &scaled
    };
    let mut buffer = vec![BACKGROUND; fw * fh];
    let (ox, oy) = ((fw - tw as usize) / 2, (fh - th as usize) / 2);
    for (x, y, p) in shown_image.enumerate_pixels() {
        let (x, y) = (x as usize, y as usize);
        let tile = (x / CHECKER + y / CHECKER) % 2;
        let bg = CHECKER_SHADES[tile];
        let a = p[3] as u32;
        let mix = |c: u8| (c as u32 * a + bg * (255 - a)) / 255;
        buffer[(oy + y) * fw + ox + x] = (mix(p[0]) << 16) | (mix(p[1]) << 8) | mix(p[2]);
    }
    Frame {
        buffer,
        size: (fw, fh),
    }
}

fn title(shown: &Shown, status: Option<&str>) -> String {
    let (w, h) = shown.image.dimensions();
    let mut title = format!("{} — {w}×{h} — BIC viewer", file_name(&shown.path));
    if let Some(status) = status {
        title += " — ";
        title += status;
    }
    title
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    )
}

fn is_bic(path: &Path) -> bool {
    path.extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("bic"))
}

fn pick_image(near: Option<&Path>) -> Option<PathBuf> {
    let mut extensions = vec!["bic"];
    extensions.extend(EXTENSIONS);
    let mut dialog = rfd::FileDialog::new()
        .set_title("Open image")
        .add_filter("Images", &extensions)
        .add_filter("All files", &["*"]);
    if let Some(dir) = near.and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file()
}

/// Asks where to save, proposing `<name>.bic` next to the current image.
fn pick_save_target(current: &Path) -> Option<PathBuf> {
    let stem = current
        .file_stem()
        .map_or_else(|| "image".into(), |s| s.to_string_lossy().into_owned());
    let mut dialog = rfd::FileDialog::new()
        .set_title("Save as BIC")
        .add_filter("BIC image", &["bic"])
        .set_file_name(format!("{stem}.bic"));
    if let Some(dir) = current.parent() {
        dialog = dialog.set_directory(dir);
    }
    let mut target = dialog.save_file()?;
    if !is_bic(&target) {
        target.set_extension("bic");
    }
    Some(target)
}

fn start_save(shown: &Shown, target: PathBuf) -> Saving {
    let (image, metadata) = (shown.image.clone(), shown.metadata.clone());
    let (sender, done) = mpsc::channel();
    let path = target.clone();
    thread::spawn(move || {
        // The receiver may be gone if the window closed; nothing to report then.
        let _ = sender.send(save_bic(&path, &image, &metadata));
    });
    Saving { target, done }
}

/// Encodes losslessly with the default (size-first) settings. The file is
/// written under a temporary name and renamed, so an interrupted save never
/// leaves a truncated `.bic` behind.
fn save_bic(target: &Path, image: &RgbaImage, metadata: &Metadata) -> Result<usize> {
    let file = encode_rgba_with_metadata(
        image.width(),
        image.height(),
        image.as_raw(),
        metadata,
        EncodeOptions::default(),
    )?;
    let partial = target.with_extension("bic.partial");
    fs::write(&partial, &file).with_context(|| format!("writing {}", partial.display()))?;
    fs::rename(&partial, target).with_context(|| format!("writing {}", target.display()))?;
    Ok(file.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_bic_reopens_with_pixels_and_profile() {
        let image = RgbaImage::from_fn(13, 7, |x, y| {
            image::Rgba([x as u8 * 9, y as u8 * 30, 7, 200])
        });
        let metadata = Metadata {
            icc_profile: Some(vec![1, 2, 3, 4]),
        };
        let dir = std::env::temp_dir().join(format!("bic-viewer-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("saved.bic");
        let bytes = save_bic(&target, &image, &metadata).unwrap();
        let reopened = open(&target, DecodeLimits::default()).unwrap();
        let leftover = target.with_extension("bic.partial").exists();
        fs::remove_dir_all(&dir).ok();
        assert_eq!(
            bytes as u64,
            fs::metadata(&target).map_or(bytes as u64, |m| m.len())
        );
        assert_eq!(reopened, (image, metadata));
        assert!(!leftover);
    }
}
