use anyhow::{bail, ensure, Context, Result};

const MAGIC: &[u8; 4] = b"BIC1";
const VERSION: u8 = 1;
const STORAGE_NATIVE: u8 = 0;
const STORAGE_PNG: u8 = 1;
const MAX_PIXELS: usize = 100_000_000;
const BLOCK_SIZES: [usize; 3] = [256, 1024, 4096];
const BLOCK_HEADER: usize = 7;
const RLE_BITS: [u8; 4] = [4, 8, 12, 16];

/// Resource caps applied before the decoder allocates anything.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub max_pixels: usize,
    /// Upper bound on the decoder's peak working memory, in bytes.
    pub max_alloc: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_pixels: MAX_PIXELS,
            max_alloc: 1 << 30,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    pub quant_step: u8,
    pub smooth_tolerance: u8,
    pub residual: bool,
    pub png_fallback: bool,
    pub zstd_level: i32,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            quant_step: 1,
            smooth_tolerance: 0,
            residual: false,
            png_fallback: false,
            zstd_level: 9,
        }
    }
}

pub fn encode_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
    options: EncodeOptions,
) -> Result<Vec<u8>> {
    let pixels = checked_pixels(width, height, MAX_PIXELS)?;
    ensure!(
        rgba.len() == pixels * 4,
        "RGBA byte count does not match dimensions"
    );
    ensure!(
        options.quant_step > 0,
        "quantization step must be at least 1"
    );
    let native = native_stream(width, height, rgba, options)?;
    let packed = zstd::stream::encode_all(native.as_slice(), options.zstd_level)?;
    let native_file = container(width, height, STORAGE_NATIVE, native.len() as u64, &packed);
    if options.png_fallback && options.quant_step == 1 && options.smooth_tolerance == 0 {
        let png = png_bytes(width, height, rgba)?;
        if png.len() <= native_file.len() {
            return Ok(container(
                width,
                height,
                STORAGE_PNG,
                png.len() as u64,
                &png,
            ));
        }
    }
    Ok(native_file)
}

pub fn decode_rgba(file: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    decode_rgba_with_limits(file, DecodeLimits::default())
}

pub fn decode_rgba_with_limits(file: &[u8], limits: DecodeLimits) -> Result<(u32, u32, Vec<u8>)> {
    let (width, height, storage, plain_len, payload) = parse_container(file, limits.max_pixels)?;
    let pixels = width as usize * height as usize;
    if storage == STORAGE_PNG {
        ensure!(
            pixels.checked_mul(4).is_some_and(|n| n <= limits.max_alloc),
            "image exceeds decode memory limit"
        );
        let mut png_limits = image::Limits::default();
        png_limits.max_image_width = Some(width);
        png_limits.max_image_height = Some(height);
        png_limits.max_alloc = Some(limits.max_alloc as u64);
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(payload), image::ImageFormat::Png);
        reader.limits(png_limits);
        let image = reader.decode()?.to_rgba8();
        ensure!(
            image.width() == width && image.height() == height,
            "PNG dimensions contradict BIC header"
        );
        return Ok((width, height, image.into_raw()));
    }
    ensure!(
        storage == STORAGE_NATIVE,
        "unsupported storage type {storage}"
    );
    // Peak working set of decode_native: RGBA output plus one plane and its
    // two residual planes, on top of the decompressed stream itself.
    let plain_len = usize::try_from(plain_len).context("plain length overflow")?;
    let needed = pixels
        .checked_mul(7)
        .and_then(|n| n.checked_add(plain_len))
        .context("decode size overflow")?;
    ensure!(
        needed <= limits.max_alloc,
        "image exceeds decode memory limit"
    );
    // Bounded decompression: a payload that inflates past plain_len fails
    // instead of growing without limit.
    let plain = zstd::bulk::decompress(payload, plain_len).context("Zstd payload")?;
    ensure!(plain.len() == plain_len, "Zstd output length mismatch");
    decode_native(width, height, &plain)
}

pub fn is_png_fallback(file: &[u8]) -> Result<bool> {
    Ok(parse_container(file, MAX_PIXELS)?.2 == STORAGE_PNG)
}

fn checked_pixels(width: u32, height: u32, max_pixels: usize) -> Result<usize> {
    let count = (width as usize)
        .checked_mul(height as usize)
        .context("dimension overflow")?;
    ensure!(count <= max_pixels, "image exceeds pixel limit");
    Ok(count)
}

fn container(width: u32, height: u32, storage: u8, plain_len: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(storage);
    put_u16(&mut out, 0);
    put_u32(&mut out, width);
    put_u32(&mut out, height);
    put_u64(&mut out, plain_len);
    put_u64(&mut out, payload.len() as u64);
    out.extend_from_slice(payload);
    out
}

fn parse_container(file: &[u8], max_pixels: usize) -> Result<(u32, u32, u8, u64, &[u8])> {
    ensure!(file.len() >= 32 && &file[..4] == MAGIC, "not a BIC1 file");
    ensure!(file[4] == VERSION, "unsupported BIC version");
    let storage = file[5];
    ensure!(file[6..8] == [0, 0], "unsupported BIC flags");
    let width = get_u32(file, 8)?;
    let height = get_u32(file, 12)?;
    checked_pixels(width, height, max_pixels)?;
    let plain_len = get_u64(file, 16)?;
    let payload_len = get_u64(file, 24)?;
    ensure!(
        (file.len() - 32) as u64 == payload_len,
        "container payload length mismatch"
    );
    Ok((width, height, storage, plain_len, &file[32..]))
}

fn native_stream(width: u32, height: u32, rgba: &[u8], opt: EncodeOptions) -> Result<Vec<u8>> {
    let pixels = checked_pixels(width, height, MAX_PIXELS)?;
    let mut out = Vec::new();
    for c in 0..4 {
        let plane: Vec<u8> = (0..pixels).map(|p| rgba[p * 4 + c]).collect();
        if plane.iter().all(|&v| v == if c == 3 { 255 } else { 0 }) {
            out.push(0);
            continue;
        }
        if plane.iter().all(|&v| v == plane[0]) {
            out.extend([1, plane[0]]);
            continue;
        }
        let min = *plane.iter().min().unwrap();
        let max = *plane.iter().max().unwrap();
        let normalized: Vec<u8> = plane.iter().map(|v| v - min).collect();
        let transformed = transform(&normalized, opt.quant_step, opt.smooth_tolerance);
        let residual: Vec<i16> = normalized
            .iter()
            .zip(&transformed)
            .map(|(&a, &b)| a as i16 - b as i16)
            .collect();
        let has_residual = opt.residual && (opt.quant_step > 1 || opt.smooth_tolerance > 0);
        out.extend([2, min, max, opt.quant_step, has_residual as u8]);
        write_plane(&mut out, &transformed)?;
        if has_residual {
            let lo: Vec<u8> = residual.iter().map(|v| *v as u16 as u8).collect();
            let hi: Vec<u8> = residual.iter().map(|v| (*v as u16 >> 8) as u8).collect();
            write_plane(&mut out, &lo)?;
            write_plane(&mut out, &hi)?;
        }
    }
    Ok(out)
}

fn decode_native(width: u32, height: u32, plain: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let pixels = width as usize * height as usize;
    let mut at = 0;
    let mut rgba = vec![0u8; pixels * 4];
    let fill = |rgba: &mut [u8], c: usize, v: u8| {
        rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|px| px[c] = v);
    };
    for c in 0..4 {
        let kind = take(plain, &mut at, 1)?[0];
        match kind {
            0 => fill(&mut rgba, c, if c == 3 { 255 } else { 0 }),
            1 => {
                let v = take(plain, &mut at, 1)?[0];
                fill(&mut rgba, c, v);
            }
            2 => {
                let min = take(plain, &mut at, 1)?[0];
                let max = take(plain, &mut at, 1)?[0];
                let _step = take(plain, &mut at, 1)?[0];
                let residual = take(plain, &mut at, 1)?[0] != 0;
                ensure!(min <= max, "channel minimum exceeds maximum");
                let range = max - min;
                let mut p = read_plane(plain, &mut at, pixels)?;
                if residual {
                    let lo = read_plane(plain, &mut at, pixels)?;
                    let hi = read_plane(plain, &mut at, pixels)?;
                    for ((v, &l), &h) in p.iter_mut().zip(&lo).zip(&hi) {
                        let d = i16::from_le_bytes([l, h]);
                        *v = (*v as i16 + d).clamp(0, 255) as u8;
                    }
                }
                ensure!(
                    p.iter().all(|&v| v <= range),
                    "normalized value exceeds range"
                );
                for (px, &v) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(&p) {
                    px[c] = v + min;
                }
            }
            _ => bail!("invalid channel descriptor {kind}"),
        }
    }
    ensure!(at == plain.len(), "trailing native stream bytes");
    Ok((width, height, rgba))
}

fn transform(input: &[u8], step: u8, tolerance: u8) -> Vec<u8> {
    let mut out = input.to_vec();
    if tolerance > 0 {
        for chunk in out.chunks_mut(256) {
            let mean = (chunk.iter().map(|&v| v as u32).sum::<u32>() / chunk.len() as u32) as u8;
            for v in chunk {
                if v.abs_diff(mean) <= tolerance {
                    *v = mean;
                }
            }
        }
    }
    let range_max = *input.iter().max().unwrap_or(&0) as u16;
    if step > 1 {
        for v in &mut out {
            *v = (((*v as u16 + step as u16 / 2) / step as u16) * step as u16).min(range_max) as u8;
        }
    }
    out
}

fn write_plane(out: &mut Vec<u8>, plane: &[u8]) -> Result<()> {
    ensure!(plane.len() <= u32::MAX as usize, "plane too large");
    let blocks = partition(plane);
    put_u32(out, blocks.len() as u32);
    for (start, end, mode, payload) in blocks {
        put_u16(out, (end - start) as u16);
        out.push(mode);
        put_u32(out, payload.len() as u32);
        out.extend(payload);
    }
    Ok(())
}
fn read_plane(data: &[u8], at: &mut usize, expected: usize) -> Result<Vec<u8>> {
    let count = read_u32(data, at)? as usize;
    ensure!(count <= expected, "too many blocks");
    let mut out = Vec::with_capacity(expected);
    for _ in 0..count {
        let len = read_u16(data, at)? as usize;
        ensure!(
            len > 0 && len <= 4096 && out.len() + len <= expected,
            "bad block length"
        );
        let mode = take(data, at, 1)?[0];
        let bytes = read_u32(data, at)? as usize;
        let payload = take(data, at, bytes)?;
        match mode {
            0 => {
                ensure!(payload.len() == len, "RAW length mismatch");
                out.extend(payload);
            }
            1..=4 => decode_rle(payload, len, RLE_BITS[(mode - 1) as usize], &mut out)?,
            _ => bail!("unknown block mode"),
        }
    }
    ensure!(out.len() == expected, "plane length mismatch");
    Ok(out)
}

/// Splits a plane into blocks with the fewest total bytes. Block boundaries
/// fall on multiples of the smallest block size, so this is a shortest path
/// over those boundaries, solved back to front.
fn partition(plane: &[u8]) -> Vec<(usize, usize, u8, Vec<u8>)> {
    let grain = BLOCK_SIZES[0];
    let k = plane.len().div_ceil(grain);
    let pos = |i: usize| (i * grain).min(plane.len());
    let mut cost = vec![usize::MAX; k + 1];
    let mut next = vec![k; k + 1];
    cost[k] = 0;
    for i in (0..k).rev() {
        for &n in &BLOCK_SIZES {
            let j = (i + n / grain).min(k);
            let c = BLOCK_HEADER + block_cost(&plane[pos(i)..pos(j)]).1 + cost[j];
            // `<=` keeps the larger block on ties: fewer blocks, same size.
            if c <= cost[i] {
                cost[i] = c;
                next[i] = j;
            }
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < k {
        let (start, end) = (pos(i), pos(next[i]));
        let (m, p) = best_mode(&plane[start..end]);
        out.push((start, end, m, p));
        i = next[i];
    }
    out
}
fn best_mode(input: &[u8]) -> (u8, Vec<u8>) {
    match block_cost(input).0 {
        0 => (0, input.to_vec()),
        m => (m, encode_rle(input, RLE_BITS[(m - 1) as usize])),
    }
}
/// Mode and payload length of the cheapest encoding, without building it.
/// Must agree with `encode_rle`'s output length.
fn block_cost(input: &[u8]) -> (u8, usize) {
    let mut runs = [0usize; 4];
    let mut at = 0;
    while at < input.len() {
        let v = input[at];
        let run = input[at..].iter().take_while(|&&x| x == v).count();
        for (r, bits) in runs.iter_mut().zip(RLE_BITS) {
            *r += run.div_ceil(1 << bits);
        }
        at += run;
    }
    let mut best = (0, input.len());
    for (i, (&r, bits)) in runs.iter().zip(RLE_BITS).enumerate() {
        let len = 2 + r + (r * bits as usize).div_ceil(8);
        if len < best.1 {
            best = (i as u8 + 1, len);
        }
    }
    best
}
fn encode_rle(input: &[u8], bits: u8) -> Vec<u8> {
    let max = (1usize << bits) - 1;
    let mut runs = Vec::new();
    let mut at = 0;
    while at < input.len() {
        let val = input[at];
        let mut run = 1;
        while at + run < input.len() && input[at + run] == val {
            run += 1
        }
        let mut left = run;
        while left > 0 {
            let chunk = left.min(max + 1);
            runs.push((val, chunk));
            left -= chunk;
        }
        at += run;
    }
    let mut out = Vec::with_capacity(2 + runs.len() * 2);
    put_u16(&mut out, runs.len() as u16);
    out.extend(runs.iter().map(|(v, _)| *v));
    let mut bw = BitWriter::default();
    for (_, count) in runs {
        bw.put(&mut out, (count - 1) as u32, bits);
    }
    bw.finish(&mut out);
    out
}
fn decode_rle(data: &[u8], len: usize, bits: u8, out: &mut Vec<u8>) -> Result<()> {
    ensure!(data.len() >= 2, "truncated RLE header");
    let runs = u16::from_le_bytes([data[0], data[1]]) as usize;
    ensure!(data.len() >= 2 + runs, "truncated RLE values");
    let values = &data[2..2 + runs];
    let mut at = 2 + runs;
    let mut br = BitReader::default();
    let target = out.len() + len;
    for &value in values {
        let count = br.get(data, &mut at, bits)? as usize + 1;
        ensure!(out.len() + count <= target, "RLE overrun");
        out.extend(std::iter::repeat_n(value, count));
    }
    ensure!(
        out.len() == target && br.remaining_zero(data, at),
        "invalid RLE length or padding"
    );
    Ok(())
}
#[derive(Default)]
struct BitWriter {
    buf: u32,
    bits: u8,
}
impl BitWriter {
    fn put(&mut self, out: &mut Vec<u8>, v: u32, n: u8) {
        self.buf |= v << self.bits;
        self.bits += n;
        while self.bits >= 8 {
            out.push(self.buf as u8);
            self.buf >>= 8;
            self.bits -= 8
        }
    }
    fn finish(&mut self, out: &mut Vec<u8>) {
        if self.bits > 0 {
            out.push(self.buf as u8)
        }
    }
}
#[derive(Default)]
struct BitReader {
    buf: u32,
    bits: u8,
}
impl BitReader {
    fn get(&mut self, data: &[u8], at: &mut usize, n: u8) -> Result<u32> {
        while self.bits < n {
            ensure!(*at < data.len(), "truncated RLE counter");
            self.buf |= (data[*at] as u32) << self.bits;
            *at += 1;
            self.bits += 8
        }
        let v = self.buf & ((1 << n) - 1);
        self.buf >>= n;
        self.bits -= n;
        Ok(v)
    }
    fn remaining_zero(&self, data: &[u8], at: usize) -> bool {
        self.buf == 0 && data[at..].iter().all(|v| *v == 0)
    }
}
fn png_bytes(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let image = image::RgbaImage::from_raw(w, h, rgba.to_vec()).context("invalid image")?;
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}
fn take<'a>(d: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = at.checked_add(n).context("offset overflow")?;
    ensure!(end <= d.len(), "truncated stream");
    let r = &d[*at..end];
    *at = end;
    Ok(r)
}
fn put_u16(o: &mut Vec<u8>, v: u16) {
    o.extend(v.to_le_bytes())
}
fn put_u32(o: &mut Vec<u8>, v: u32) {
    o.extend(v.to_le_bytes())
}
fn put_u64(o: &mut Vec<u8>, v: u64) {
    o.extend(v.to_le_bytes())
}
fn get_u32(d: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(take_at(d, p, 4)?.try_into().unwrap()))
}
fn get_u64(d: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(take_at(d, p, 8)?.try_into().unwrap()))
}
fn take_at(d: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    ensure!(p + n <= d.len(), "truncated header");
    Ok(&d[p..p + n])
}
fn read_u16(d: &[u8], a: &mut usize) -> Result<u16> {
    Ok(u16::from_le_bytes(take(d, a, 2)?.try_into().unwrap()))
}
fn read_u32(d: &[u8], a: &mut usize) -> Result<u32> {
    Ok(u32::from_le_bytes(take(d, a, 4)?.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_roundtrip_mixed() {
        let mut p = Vec::new();
        for y in 0..37u8 {
            for x in 0..61u8 {
                p.extend([x, y, if x < 30 { 8 } else { 200 }, 255]);
            }
        }
        let f = encode_rgba(61, 37, &p, EncodeOptions::default()).unwrap();
        assert_eq!(decode_rgba(&f).unwrap().2, p);
    }
    #[test]
    fn residue_makes_transform_exact() {
        let p = (0..(64 * 64))
            .flat_map(|x| [x as u8, (x / 3) as u8, 9, 255])
            .collect::<Vec<_>>();
        let f = encode_rgba(
            64,
            64,
            &p,
            EncodeOptions {
                quant_step: 7,
                smooth_tolerance: 5,
                residual: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(decode_rgba(&f).unwrap().2, p)
    }
    #[test]
    fn lossy_narrow_range_decodes() {
        let p = (0..256)
            .flat_map(|i| [100 + (i % 10) as u8, 99, 98, 255])
            .collect::<Vec<_>>();
        let f = encode_rgba(
            16,
            16,
            &p,
            EncodeOptions {
                quant_step: 7,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(decode_rgba(&f).unwrap().2.len(), p.len());
    }
    /// Deterministic mix of flat runs, gradients and noise.
    fn mixed_plane(n: usize) -> Vec<u8> {
        let mut seed = 0x2545_f491u32;
        (0..n)
            .map(|i| match (i / 3000) % 3 {
                0 => 40,
                1 => (i / 7) as u8,
                _ => {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                }
            })
            .collect()
    }
    fn native_file(width: u32, height: u32, plain: &[u8]) -> Vec<u8> {
        let z = zstd::stream::encode_all(plain, 3).unwrap();
        container(width, height, STORAGE_NATIVE, plain.len() as u64, &z)
    }
    #[test]
    fn partition_uses_large_blocks_for_long_runs() {
        let mut plane = vec![0u8; 4096];
        plane.extend([255u8; 4096]);
        let blocks = partition(&plane);
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|b| b.1 - b.0 == 4096));
    }
    #[test]
    fn partition_never_worse_than_fixed_small_blocks() {
        let plane = mixed_plane(20_000);
        let size = |blocks: &[(usize, usize, u8, Vec<u8>)]| {
            blocks
                .iter()
                .map(|b| BLOCK_HEADER + b.3.len())
                .sum::<usize>()
        };
        let fixed: Vec<_> = plane
            .chunks(BLOCK_SIZES[0])
            .map(|c| {
                let (m, p) = best_mode(c);
                (0, c.len(), m, p)
            })
            .collect();
        assert!(size(&partition(&plane)) <= size(&fixed));
        let mut out = Vec::new();
        write_plane(&mut out, &plane).unwrap();
        assert_eq!(read_plane(&out, &mut 0, plane.len()).unwrap(), plane);
    }
    #[test]
    fn block_cost_matches_encoded_length() {
        let plane = mixed_plane(12_000);
        for block in plane
            .chunks(4096)
            .chain([&[9u8; 70_000][..4096], &[1, 2][..]])
        {
            let (mode, len) = block_cost(block);
            let (built_mode, payload) = best_mode(block);
            assert_eq!((built_mode, payload.len()), (mode, len));
            for bits in RLE_BITS {
                let runs: usize = {
                    let mut r = 0;
                    let mut at = 0;
                    while at < block.len() {
                        let run = block[at..].iter().take_while(|&&x| x == block[at]).count();
                        r += run.div_ceil(1 << bits);
                        at += run;
                    }
                    r
                };
                let expected = 2 + runs + (runs * bits as usize).div_ceil(8);
                assert_eq!(encode_rle(block, bits).len(), expected);
            }
        }
    }
    #[test]
    fn rejects_channel_min_above_max() {
        // R: variable, min=200, max=10, one RAW block holding a single 0.
        let mut plain = vec![2u8, 200, 10, 1, 0];
        plain.extend(1u32.to_le_bytes());
        plain.extend(1u16.to_le_bytes());
        plain.push(0);
        plain.extend(1u32.to_le_bytes());
        plain.push(0);
        plain.extend([0, 0, 0]);
        let err = decode_rgba(&native_file(1, 1, &plain)).unwrap_err();
        assert!(err.to_string().contains("minimum exceeds maximum"), "{err}");
    }
    #[test]
    fn rejects_payload_inflating_past_declared_length() {
        let bomb = zstd::stream::encode_all(&vec![0u8; 10_000_000][..], 3).unwrap();
        let file = container(1, 1, STORAGE_NATIVE, 64, &bomb);
        assert!(decode_rgba(&file).is_err());
    }
    #[test]
    fn rejects_dimensions_over_memory_limit_before_allocating() {
        let file = native_file(8192, 8192, &[0, 0, 0, 0]);
        let limits = DecodeLimits {
            max_alloc: 1 << 20,
            ..Default::default()
        };
        let err = decode_rgba_with_limits(&file, limits).unwrap_err();
        assert!(err.to_string().contains("memory limit"), "{err}");
        // Within the default limit the same all-absent stream is valid.
        assert_eq!(
            decode_rgba(&native_file(64, 64, &[0, 0, 0, 0]))
                .unwrap()
                .2
                .len(),
            64 * 64 * 4
        );
    }
    #[test]
    fn rejects_nonzero_flags() {
        let mut file = native_file(1, 1, &[0, 0, 0, 0]);
        file[6] = 1;
        assert!(decode_rgba(&file).is_err());
    }
    #[test]
    fn png_fallback_respects_memory_limit() {
        let p = vec![7u8; 32 * 32 * 4];
        let png = png_bytes(32, 32, &p).unwrap();
        let file = container(32, 32, STORAGE_PNG, png.len() as u64, &png);
        let tight = DecodeLimits {
            max_alloc: 1024,
            ..Default::default()
        };
        assert!(decode_rgba_with_limits(&file, tight).is_err());
        assert_eq!(decode_rgba(&file).unwrap().2, p);
        // A PNG larger than the header claims is refused by the PNG reader.
        let big = png_bytes(64, 64, &vec![7u8; 64 * 64 * 4]).unwrap();
        let lying = container(32, 32, STORAGE_PNG, big.len() as u64, &big);
        assert!(decode_rgba(&lying).is_err());
    }
    #[test]
    fn rle_widths_roundtrip() {
        for bits in [4, 8, 12, 16] {
            let p = vec![7; 5000];
            let e = encode_rle(&p, bits);
            let mut o = Vec::new();
            decode_rle(&e, p.len(), bits, &mut o).unwrap();
            assert_eq!(o, p)
        }
    }
}
