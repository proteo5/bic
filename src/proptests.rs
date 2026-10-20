//! Property-based tests: roundtrip and error-bound guarantees for generated
//! images, and
//! "never panics" for mutated streams (a stable-toolchain stand-in for the
//! cargo-fuzz targets in `fuzz/`). Each property runs 256 cases by default;
//! set `PROPTEST_CASES` for a longer run.

use super::*;
use proptest::prelude::*;

/// Images up to 40×40 (including empty ones) that mix long runs, noise,
/// opaque alpha and constant channels, so every channel descriptor and
/// block mode is exercised.
fn image() -> impl Strategy<Value = (u32, u32, Vec<u8>)> {
    (0u32..=40, 0u32..=40).prop_flat_map(|(w, h)| {
        let n = (w * h) as usize;
        (
            Just((w, h)),
            prop::collection::vec((any::<[u8; 4]>(), 1usize..300), 1..12),
            prop::collection::vec(any::<u8>(), n * 4),
            0u8..3,
            any::<bool>(),
            prop::option::of(0usize..4),
        )
            .prop_map(move |((w, h), runs, noise, kind, opaque, flat)| {
                let mut p = Vec::with_capacity(n * 4);
                let mut run = runs
                    .iter()
                    .cycle()
                    .flat_map(|&(px, len)| std::iter::repeat_n(px, len));
                for i in 0..n {
                    let noisy =
                        kind == 1 || (kind == 2 && (i / w.max(1) as usize).is_multiple_of(2));
                    let px = if noisy {
                        noise[i * 4..i * 4 + 4].try_into().unwrap()
                    } else {
                        run.next().unwrap()
                    };
                    p.extend(px);
                }
                for (i, v) in p.iter_mut().enumerate() {
                    if opaque && i % 4 == 3 {
                        *v = 255;
                    }
                    if flat == Some(i % 4) {
                        *v = 17;
                    }
                }
                (w, h, p)
            })
    })
}

/// Planes made of runs, so RLE modes and large blocks are competitive.
fn plane() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec((any::<u8>(), 1usize..600, any::<bool>()), 1..40).prop_map(|runs| {
        let mut seed = 1u32;
        let mut out = Vec::new();
        for (v, len, noisy) in runs {
            for _ in 0..len {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                out.push(if noisy { (seed >> 24) as u8 } else { v });
            }
        }
        out
    })
}

/// Applies byte overwrites, then truncates or extends.
fn mutate(mut data: Vec<u8>, edits: &[(usize, u8)], cut: Option<usize>, tail: &[u8]) -> Vec<u8> {
    if !data.is_empty() {
        for &(i, v) in edits {
            let i = i % data.len();
            data[i] = v;
        }
    }
    if let Some(cut) = cut {
        data.truncate(cut % (data.len() + 1));
    }
    data.extend_from_slice(tail);
    data
}

fn uniform_partition_size(plane: &[u8], block: usize) -> usize {
    plane
        .chunks(block)
        .map(|c| BLOCK_HEADER + block_cost(c).1)
        .sum()
}

const SMALL: DecodeLimits = DecodeLimits {
    max_pixels: 1 << 16,
    max_alloc: 4 << 20,
};

proptest! {
    #[test]
    fn lossless_roundtrip((w, h, p) in image(), zstd_level in 1i32..=6, png_fallback: bool) {
        let options = EncodeOptions { zstd_level, png_fallback, ..Default::default() };
        let file = encode_rgba(w, h, &p, options).unwrap();
        prop_assert_eq!(decode_rgba(&file).unwrap(), (w, h, p));
    }

    #[test]
    fn near_lossless_error_is_bounded((w, h, p) in image(), max_error in 0u8..=60) {
        let options = EncodeOptions { max_error, ..Default::default() };
        let (dw, dh, d) = decode_rgba(&encode_rgba(w, h, &p, options).unwrap()).unwrap();
        prop_assert_eq!((dw, dh, d.len()), (w, h, p.len()));
        for (i, (a, b)) in p.iter().zip(&d).enumerate() {
            prop_assert!(a.abs_diff(*b) <= max_error, "sample {} differs by {} > {}", i, a.abs_diff(*b), max_error);
        }
    }

    /// Every candidate the encoder may pick, not only the smallest one.
    #[test]
    fn every_candidate_meets_its_error_bound((w, h, p) in image(), max_error in prop_oneof![Just(0u8), 1u8..=60]) {
        let (palette, list) = candidates(&p, max_error);
        for (transform, predictor) in list {
            let plain = native_stream(w as usize, &p, palette.as_ref(), transform, predictor, max_error).unwrap();
            let (_, _, d) = decode_native(w, h, &plain).unwrap();
            prop_assert_eq!(d.len(), p.len());
            prop_assert!(p.iter().zip(&d).all(|(a, b)| a.abs_diff(*b) <= max_error), "{:?}/{:?}", transform, predictor);
        }
    }

    #[test]
    fn partition_is_no_worse_than_any_uniform_split(plane in plane()) {
        let best: usize = partition(&plane).iter().map(|b| BLOCK_HEADER + b.3.len()).sum();
        for block in BLOCK_SIZES {
            prop_assert!(best <= uniform_partition_size(&plane, block));
        }
        let mut stream = Vec::new();
        write_plane(&mut stream, &plane).unwrap();
        prop_assert_eq!(read_plane(&stream, &mut 0, plane.len()).unwrap(), plane);
    }

    #[test]
    fn block_cost_matches_encoding(block in plane().prop_map(|mut p| { p.truncate(4096); p })) {
        let (mode, len) = block_cost(&block);
        let (built_mode, payload) = best_mode(&block);
        prop_assert_eq!((built_mode, payload.len()), (mode, len));
    }

    #[test]
    fn mutated_native_stream_never_panics(
        (w, h, p) in image(),
        max_error in prop_oneof![Just(0u8), 1u8..=60],
        pick: bool,
        edits in prop::collection::vec((any::<usize>(), any::<u8>()), 0..6),
        cut in prop::option::of(any::<usize>()),
        tail in prop::collection::vec(any::<u8>(), 0..4),
    ) {
        // Mutating the plain stream (not the Zstd bytes) reaches the parser.
        let (palette, list) = candidates(&p, max_error);
        let (transform, predictor) = list[pick as usize];
        let plain = native_stream(w as usize, &p, palette.as_ref(), transform, predictor, max_error).unwrap();
        let _ = decode_native(w, h, &mutate(plain, &edits, cut, &tail));
    }

    #[test]
    fn mutated_container_never_panics(
        (w, h, p) in image(),
        png_fallback: bool,
        edits in prop::collection::vec((any::<usize>(), any::<u8>()), 0..6),
        cut in prop::option::of(any::<usize>()),
        tail in prop::collection::vec(any::<u8>(), 0..4),
    ) {
        let options = EncodeOptions { png_fallback, ..Default::default() };
        let file = encode_rgba(w, h, &p, options).unwrap();
        let _ = decode_rgba_with_limits(&mutate(file, &edits, cut, &tail), SMALL);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = decode_rgba_with_limits(&bytes, SMALL);
        let mut file = MAGIC.to_vec();
        file.extend(&bytes);
        let _ = decode_rgba_with_limits(&file, SMALL);
    }
}
