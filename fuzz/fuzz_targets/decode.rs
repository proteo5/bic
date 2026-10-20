//! Arbitrary bytes as a whole BIC file. Most inputs stop at the header or in
//! Zstandard; `decode_plain` reaches the plane parser more directly.
#![no_main]

use bic::{decode_rgba_with_limits, DecodeLimits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = DecodeLimits {
        max_pixels: 1 << 16,
        max_alloc: 8 << 20,
    };
    let _ = decode_rgba_with_limits(data, limits);
});
