//! Arbitrary bytes as the native plane stream, wrapped in a valid container
//! so every input reaches the channel and block parser.
#![no_main]

use bic::{decode_rgba_with_limits, DecodeLimits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [w, h, plain @ ..] = data else { return };
    let (width, height) = (u32::from(*w % 64), u32::from(*h % 64));
    let Ok(payload) = zstd::bulk::compress(plain, 1) else {
        return;
    };

    // BIC1 header, see FORMAT.md: magic, version, storage, flags, width,
    // height, plain_len, payload_len.
    let mut file = b"BIC1".to_vec();
    file.extend([1, 0, 0, 0]);
    file.extend(width.to_le_bytes());
    file.extend(height.to_le_bytes());
    file.extend((plain.len() as u64).to_le_bytes());
    file.extend((payload.len() as u64).to_le_bytes());
    file.extend(payload);

    let limits = DecodeLimits {
        max_pixels: 1 << 16,
        max_alloc: 8 << 20,
    };
    if let Ok((w, h, rgba)) = decode_rgba_with_limits(&file, limits) {
        assert_eq!((w, h), (width, height));
        assert_eq!(rgba.len(), (width * height * 4) as usize);
    }
});
