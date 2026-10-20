//! Arbitrary images and encoder options: every decoded sample must be within
//! `max_error` of the original, and lossless encodes must be exact.
#![no_main]

use arbitrary::Arbitrary;
use bic::{decode_rgba, encode_rgba, EncodeOptions};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    width: u8,
    height: u8,
    max_error: u8,
    png_fallback: bool,
    pixels: Vec<u8>,
}

fuzz_target!(|input: Input| {
    let (w, h) = (u32::from(input.width % 48), u32::from(input.height % 48));
    let n = (w * h * 4) as usize;
    if input.pixels.is_empty() && n > 0 {
        return;
    }
    // Repeat the fuzzer's bytes to fill the image; repetition also creates runs.
    let rgba: Vec<u8> = input.pixels.iter().copied().cycle().take(n).collect();
    let options = EncodeOptions {
        max_error: input.max_error,
        png_fallback: input.png_fallback,
        zstd_level: 1,
    };
    let file = encode_rgba(w, h, &rgba, options).expect("encode");
    let (dw, dh, decoded) = decode_rgba(&file).expect("decode of own output");
    assert_eq!((dw, dh), (w, h));
    assert!(rgba
        .iter()
        .zip(&decoded)
        .all(|(a, b)| a.abs_diff(*b) <= options.max_error));
});
