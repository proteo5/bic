//! Writes the decoded pixels of a BIC file as raw RGBA. The JavaScript runtime
//! is tested against this output, so the Rust decoder stays the reference.
//!
//! Output: width (u32 little-endian), height (u32 little-endian), then
//! width × height × 4 bytes of RGBA.
//!
//! cargo run --example dump_rgba -- input.bic output.raw

use anyhow::{ensure, Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(args.len() == 3, "usage: dump_rgba <input.bic> <output.raw>");
    let file = std::fs::read(&args[1]).with_context(|| format!("reading {}", args[1]))?;
    let (width, height, rgba) = bic::decode_rgba(&file)?;
    let mut out = Vec::with_capacity(8 + rgba.len());
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out.extend(rgba);
    std::fs::write(&args[2], out).with_context(|| format!("writing {}", args[2]))?;
    Ok(())
}
