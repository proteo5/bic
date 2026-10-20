//! Console-less image viewer, used by the Windows `.bic` file association.
//! Errors are shown in a dialog because there is no terminal to print to.
#![cfg_attr(windows, windows_subsystem = "windows")]

// The viewer only needs part of the loader (no conversion notes).
#[allow(dead_code)]
#[path = "../load.rs"]
mod load;
#[path = "../viewer.rs"]
mod viewer;

use bic::DecodeLimits;
use std::path::PathBuf;

fn main() {
    // Without an argument (e.g. started from the Start menu) the viewer asks
    // for an image to open.
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    if let Err(e) = viewer::view(path.as_deref(), DecodeLimits::default()) {
        let what = path.map_or_else(|| "the image".into(), |p| p.display().to_string());
        viewer::error_dialog(&format!("Could not open {what}:\n\n{e:#}"));
        std::process::exit(1);
    }
}
