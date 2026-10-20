//! Arbitrary images and encoder options: lossless and residual encodes must
//! decode exactly; lossy encodes must stay within the documented error.
#![no_main]

use arbitrary::Arbitrary;
use bic::{decode_rgba, encode_rgba, EncodeOptions};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    width: u8,
    height: u8,
    quant_step: u8,
    smooth_tolerance: u8,
    residual: bool,
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
        quant_step: input.quant_step.max(1),
        smooth_tolerance: input.smooth_tolerance % 32,
        residual: input.residual,
        png_fallback: input.png_fallback,
        zstd_level: 1,
    };
    let file = encode_rgba(w, h, &rgba, options).expect("encode");
    let (dw, dh, decoded) = decode_rgba(&file).expect("decode of own output");
    assert_eq!((dw, dh), (w, h));
    let lossy = options.quant_step > 1 || options.smooth_tolerance > 0;
    if !lossy || options.residual {
        assert_eq!(decoded, rgba);
    } else {
        let bound = options.smooth_tolerance + options.quant_step / 2;
        assert!(rgba
            .iter()
            .zip(&decoded)
            .all(|(a, b)| a.abs_diff(*b) <= bound));
    }
});
