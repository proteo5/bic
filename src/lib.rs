use anyhow::{bail, ensure, Context, Result};
use std::collections::HashMap;

mod source;
pub use source::{embeddable_format, read_source, EmbeddableFormat, Source};

const MAGIC: &[u8; 4] = b"BIC2";
const VERSION: u8 = 2;
const STORAGE_NATIVE: u8 = 0;
const STORAGE_PNG: u8 = 1;
/// The source file (PNG, JPEG, WebP or HEIF) stored unchanged.
const STORAGE_ORIGINAL: u8 = 2;
const MAX_PIXELS: usize = 100_000_000;
const MAX_PALETTE: usize = 256;
const HEADER_LEN: usize = 32;
/// Container flag: an ICC profile follows the header.
const FLAG_ICC: u16 = 1;
const MAX_ICC: usize = 16 << 20;
const BLOCK_SIZES: [usize; 3] = [256, 1024, 4096];
const BLOCK_HEADER: usize = 7;
const RLE_BITS: [u8; 4] = [4, 8, 12, 16];
/// Green is coded first so that red and blue can be predicted from it.
const CHANNEL_ORDER: [usize; 4] = [1, 0, 2, 3];
const CHANNEL_CONSTANT: u8 = 0;
const CHANNEL_CODED: u8 = 1;

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
            // Same default as the `image` crate's decoders.
            max_alloc: 512 << 20,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    /// Largest allowed difference between an original and a decoded sample.
    /// 0 is lossless.
    pub max_error: u8,
    /// Store a PNG instead when it is smaller (lossless encodes only).
    pub png_fallback: bool,
    /// Only use predictors the decoder can vectorize (none and up), giving up
    /// per-row prediction: somewhat larger files on photographs, about twice
    /// as fast to decode.
    pub fast_decode: bool,
    /// When encoding a file (`encode_file`, `encode_source`), store the file
    /// itself if it is smaller than BIC's own encoding, so the result is never
    /// larger than the original plus the 32-byte header. Lossy originals such
    /// as JPEG are usually kept this way.
    pub keep_original: bool,
    pub zstd_level: i32,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            max_error: 0,
            png_fallback: false,
            fast_decode: false,
            keep_original: true,
            zstd_level: 19,
        }
    }
}

/// How channels relate before prediction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transform {
    Direct = 0,
    /// Red and blue are predicted relative to the decoded green.
    SubtractGreen = 1,
    /// A single plane of indices into a color table.
    Palette = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Predictor {
    /// `RowPredictor::None` on every row.
    None = 0,
    /// `RowPredictor::Up` on every row.
    Up = 1,
    /// A `RowPredictor` per row, stored before the symbols.
    PerRow = 2,
}

struct Palette {
    colors: Vec<[u8; 4]>,
    indices: Vec<u8>,
}

/// Near-lossless quantizer in the style of JPEG-LS. Prediction residuals are
/// quantized with step `2 * near + 1` and reduced modulo `range`, so every
/// symbol fits in a byte and every decoded sample is within `near` of the
/// original. With `near = 0` it is plain modulo-256 residual coding.
#[derive(Clone, Copy, Debug)]
struct Quantizer {
    near: i32,
    step: i32,
    range: i32,
}

impl Quantizer {
    fn new(max_error: u8) -> Self {
        let near = max_error as i32;
        let step = 2 * near + 1;
        Self {
            near,
            step,
            range: (255 + 2 * near) / step + 1,
        }
    }

    fn symbol(&self, x: u8, prediction: i32) -> u8 {
        let e = x as i32 - prediction;
        let q = if e >= 0 {
            (e + self.near) / self.step
        } else {
            -((self.near - e) / self.step)
        };
        q.rem_euclid(self.range) as u8
    }

    /// The signed quantized residual a symbol stands for.
    fn signed(&self, symbol: u8) -> i32 {
        let s = symbol as i32;
        if s >= (self.range + 1) / 2 {
            s - self.range
        } else {
            s
        }
    }

    fn reconstruct(&self, prediction: i32, symbol: u8) -> u8 {
        let mut r = prediction + self.signed(symbol) * self.step;
        if r < -self.near {
            r += self.range * self.step;
        } else if r > 255 + self.near {
            r -= self.range * self.step;
        }
        r.clamp(0, 255) as u8
    }
}

/// Metadata stored alongside the pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// ICC color profile of the pixels, stored as is. BIC applies no color
    /// management itself.
    pub icc_profile: Option<Vec<u8>>,
}

/// A decoded image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// 8-bit RGBA in row-major order.
    pub rgba: Vec<u8>,
    pub metadata: Metadata,
}

/// How a BIC file stores its image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// BIC's own planar coding.
    Native,
    /// A PNG of the pixels (`EncodeOptions::png_fallback`).
    PngFallback,
    /// The source file, unchanged (`EncodeOptions::keep_original`).
    Original,
}

/// Encodes an image file (any format `read_source` reads). See
/// `EncodeOptions::keep_original`.
pub fn encode_file(bytes: &[u8], options: EncodeOptions) -> Result<Vec<u8>> {
    let source = read_source(bytes, DecodeLimits::default())?;
    encode_source(&source, bytes, options)
}

/// Encodes a source already read from `original` with `read_source`.
pub fn encode_source(source: &Source, original: &[u8], options: EncodeOptions) -> Result<Vec<u8>> {
    let native = encode_rgba_with_metadata(
        source.width,
        source.height,
        &source.rgba,
        &source.metadata,
        options,
    )?;
    if options.keep_original
        && source.embeddable.is_some()
        && HEADER_LEN + original.len() <= native.len()
    {
        // The embedded file carries its own ICC profile and orientation.
        return Ok(container_with_icc(
            source.width,
            source.height,
            STORAGE_ORIGINAL,
            0,
            None,
            original,
        ));
    }
    Ok(native)
}

pub fn encode_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
    options: EncodeOptions,
) -> Result<Vec<u8>> {
    encode_rgba_with_metadata(width, height, rgba, &Metadata::default(), options)
}

pub fn encode_rgba_with_metadata(
    width: u32,
    height: u32,
    rgba: &[u8],
    metadata: &Metadata,
    options: EncodeOptions,
) -> Result<Vec<u8>> {
    let pixels = checked_pixels(width, height, MAX_PIXELS)?;
    ensure!(
        rgba.len() == pixels * 4,
        "RGBA byte count does not match dimensions"
    );
    let icc = metadata.icc_profile.as_deref();
    if let Some(icc) = icc {
        ensure!(icc.len() <= MAX_ICC, "ICC profile exceeds {MAX_ICC} bytes");
    }
    let (palette, candidates) = candidates(rgba, options.max_error, options.fast_decode);
    let encode = |(transform, predictor, error): Candidate| -> Result<(usize, Vec<u8>)> {
        let plain = native_stream(
            width as usize,
            rgba,
            palette.as_ref(),
            transform,
            predictor,
            error,
        )?;
        Ok((
            plain.len(),
            zstd::bulk::compress(&plain, options.zstd_level)?,
        ))
    };
    // Candidates are independent, so they are encoded in parallel.
    let results: Vec<Result<(usize, Vec<u8>)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = candidates
            .into_iter()
            .map(|c| scope.spawn(move || encode(c)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("encoder thread panicked"))
            .collect()
    });
    let mut best: Option<(usize, Vec<u8>)> = None;
    for result in results {
        let (plain_len, packed) = result?;
        if best.as_ref().is_none_or(|b| packed.len() < b.1.len()) {
            best = Some((plain_len, packed));
        }
    }
    let (plain_len, packed) = best.context("no encoding candidate")?;
    // PNG cannot represent an empty image, so those always stay native.
    if options.png_fallback && options.max_error == 0 && pixels > 0 {
        let png = png_bytes(width, height, rgba)?;
        if png.len() <= packed.len() {
            let len = png.len() as u64;
            return Ok(container_with_icc(
                width,
                height,
                STORAGE_PNG,
                len,
                icc,
                &png,
            ));
        }
    }
    let len = plain_len as u64;
    Ok(container_with_icc(
        width,
        height,
        STORAGE_NATIVE,
        len,
        icc,
        &packed,
    ))
}

/// One way to encode an image.
type Candidate = (Transform, Predictor, u8);

/// The encodings to try for this image: every valid transform and predictor
/// combination (without per-row prediction when `fast_decode` is set). For a
/// near-lossless request the lossless encodings are tried too, since an exact
/// file meets any error bound and a palette can beat lossy coding on flat
/// graphics. Encoding time is not a priority, so the encoder simply keeps
/// whichever is smallest.
fn candidates(rgba: &[u8], max_error: u8, fast_decode: bool) -> (Option<Palette>, Vec<Candidate>) {
    let palette = build_palette(rgba);
    let predictors: &[Predictor] = if fast_decode {
        &[Predictor::None, Predictor::Up]
    } else {
        &[Predictor::None, Predictor::Up, Predictor::PerRow]
    };
    let errors: &[u8] = if max_error > 0 { &[max_error, 0] } else { &[0] };
    let mut list = Vec::new();
    for &error in errors {
        let mut transforms = vec![Transform::Direct, Transform::SubtractGreen];
        if error == 0 && palette.is_some() {
            transforms.push(Transform::Palette);
        }
        for transform in transforms {
            list.extend(predictors.iter().map(|&p| (transform, p, error)));
        }
    }
    (palette, list)
}

/// Indexes images with at most 256 distinct colors, most frequent first.
fn build_palette(rgba: &[u8]) -> Option<Palette> {
    let pixels = rgba.as_chunks::<4>().0;
    let mut counts: HashMap<[u8; 4], usize> = HashMap::new();
    for px in pixels {
        *counts.entry(*px).or_default() += 1;
        if counts.len() > MAX_PALETTE {
            return None;
        }
    }
    if counts.is_empty() {
        return None;
    }
    let mut colors: Vec<([u8; 4], usize)> = counts.into_iter().collect();
    // Ties are broken by value so the output is deterministic.
    colors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let lookup: HashMap<[u8; 4], u8> = colors
        .iter()
        .enumerate()
        .map(|(i, c)| (c.0, i as u8))
        .collect();
    Some(Palette {
        indices: pixels.iter().map(|px| lookup[px]).collect(),
        colors: colors.into_iter().map(|c| c.0).collect(),
    })
}

pub fn decode_rgba(file: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    decode_rgba_with_limits(file, DecodeLimits::default())
}

pub fn decode_rgba_with_limits(file: &[u8], limits: DecodeLimits) -> Result<(u32, u32, Vec<u8>)> {
    let d = decode_with_metadata(file, limits)?;
    Ok((d.width, d.height, d.rgba))
}

/// Decodes pixels and metadata.
pub fn decode_with_metadata(file: &[u8], limits: DecodeLimits) -> Result<Decoded> {
    let c = parse_container(file, limits.max_pixels)?;
    let (width, height) = (c.width, c.height);
    let pixels = width as usize * height as usize;
    let metadata = Metadata {
        icc_profile: c.icc.map(|icc| icc.to_vec()),
    };
    let icc_len = c.icc.map_or(0, |icc| icc.len());
    if c.storage == STORAGE_ORIGINAL {
        ensure!(
            pixels.checked_mul(4).is_some_and(|n| n <= limits.max_alloc),
            "image exceeds decode memory limit"
        );
        // Only formats every build can decode may be embedded.
        ensure!(
            embeddable_format(c.payload).is_some(),
            "embedded file is not PNG, JPEG, WebP or HEIF"
        );
        let source = read_source(c.payload, limits).context("embedded original")?;
        ensure!(
            (source.width, source.height) == (width, height),
            "embedded image dimensions contradict BIC header"
        );
        let metadata = if metadata.icc_profile.is_some() {
            metadata
        } else {
            source.metadata
        };
        return Ok(Decoded {
            width,
            height,
            rgba: source.rgba,
            metadata,
        });
    }
    if c.storage == STORAGE_PNG {
        ensure!(
            pixels
                .checked_mul(4)
                .and_then(|n| n.checked_add(icc_len))
                .is_some_and(|n| n <= limits.max_alloc),
            "image exceeds decode memory limit"
        );
        let mut png_limits = image::Limits::default();
        png_limits.max_image_width = Some(width);
        png_limits.max_image_height = Some(height);
        png_limits.max_alloc = Some(limits.max_alloc as u64);
        let mut reader = image::ImageReader::with_format(
            std::io::Cursor::new(c.payload),
            image::ImageFormat::Png,
        );
        reader.limits(png_limits);
        let image = reader.decode()?.to_rgba8();
        ensure!(
            image.width() == width && image.height() == height,
            "PNG dimensions contradict BIC header"
        );
        return Ok(Decoded {
            width,
            height,
            rgba: image.into_raw(),
            metadata,
        });
    }
    ensure!(
        c.storage == STORAGE_NATIVE,
        "unsupported storage type {}",
        c.storage
    );
    // Peak working set of decode_native: RGBA output, the decoded green plane
    // and the plane being decoded, on top of the decompressed stream itself.
    let plain_len = usize::try_from(c.plain_len).context("plain length overflow")?;
    let needed = pixels
        .checked_mul(6)
        .and_then(|n| n.checked_add(plain_len))
        .and_then(|n| n.checked_add(icc_len))
        .context("decode size overflow")?;
    ensure!(
        needed <= limits.max_alloc,
        "image exceeds decode memory limit"
    );
    // Bounded decompression: a payload that inflates past plain_len fails
    // instead of growing without limit.
    let plain = zstd::bulk::decompress(c.payload, plain_len).context("Zstd payload")?;
    ensure!(plain.len() == plain_len, "Zstd output length mismatch");
    let (_, _, rgba) = decode_native(width, height, &plain)?;
    Ok(Decoded {
        width,
        height,
        rgba,
        metadata,
    })
}

/// How `file` stores its image, from the header alone.
pub fn storage(file: &[u8]) -> Result<Storage> {
    match parse_container(file, MAX_PIXELS)?.storage {
        STORAGE_NATIVE => Ok(Storage::Native),
        STORAGE_PNG => Ok(Storage::PngFallback),
        STORAGE_ORIGINAL => Ok(Storage::Original),
        other => bail!("unsupported storage type {other}"),
    }
}

fn checked_pixels(width: u32, height: u32, max_pixels: usize) -> Result<usize> {
    let count = (width as usize)
        .checked_mul(height as usize)
        .context("dimension overflow")?;
    ensure!(count <= max_pixels, "image exceeds pixel limit");
    Ok(count)
}

#[cfg(test)]
fn container(width: u32, height: u32, storage: u8, plain_len: u64, payload: &[u8]) -> Vec<u8> {
    container_with_icc(width, height, storage, plain_len, None, payload)
}

fn container_with_icc(
    width: u32,
    height: u32,
    storage: u8,
    plain_len: u64,
    icc: Option<&[u8]>,
    payload: &[u8],
) -> Vec<u8> {
    let icc_bytes = icc.map_or(0, |i| 4 + i.len());
    let mut out = Vec::with_capacity(HEADER_LEN + icc_bytes + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(storage);
    put_u16(&mut out, if icc.is_some() { FLAG_ICC } else { 0 });
    put_u32(&mut out, width);
    put_u32(&mut out, height);
    put_u64(&mut out, plain_len);
    put_u64(&mut out, payload.len() as u64);
    if let Some(icc) = icc {
        put_u32(&mut out, icc.len() as u32);
        out.extend_from_slice(icc);
    }
    out.extend_from_slice(payload);
    out
}

struct Container<'a> {
    width: u32,
    height: u32,
    storage: u8,
    plain_len: u64,
    icc: Option<&'a [u8]>,
    payload: &'a [u8],
}

fn parse_container(file: &[u8], max_pixels: usize) -> Result<Container<'_>> {
    ensure!(
        file.len() >= HEADER_LEN && &file[..4] == MAGIC,
        "not a BIC2 file"
    );
    ensure!(file[4] == VERSION, "unsupported BIC version");
    let storage = file[5];
    let flags = u16::from_le_bytes([file[6], file[7]]);
    ensure!(flags & !FLAG_ICC == 0, "unsupported BIC flags");
    let width = get_u32(file, 8)?;
    let height = get_u32(file, 12)?;
    checked_pixels(width, height, max_pixels)?;
    let plain_len = get_u64(file, 16)?;
    let payload_len = get_u64(file, 24)?;
    let mut at = HEADER_LEN;
    let icc = if flags & FLAG_ICC != 0 {
        let len = read_u32(file, &mut at)? as usize;
        ensure!(len <= MAX_ICC, "ICC profile exceeds {MAX_ICC} bytes");
        Some(take(file, &mut at, len).context("truncated ICC profile")?)
    } else {
        None
    };
    ensure!(
        (file.len() - at) as u64 == payload_len,
        "container payload length mismatch"
    );
    Ok(Container {
        width,
        height,
        storage,
        plain_len,
        icc,
        payload: &file[at..],
    })
}

/// Prediction rule for one row of samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowPredictor {
    None = 0,
    Left = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
    /// Median edge detector from LOCO-I / JPEG-LS.
    Med = 5,
}

const ROW_PREDICTORS: [RowPredictor; 6] = [
    RowPredictor::None,
    RowPredictor::Left,
    RowPredictor::Up,
    RowPredictor::Average,
    RowPredictor::Paeth,
    RowPredictor::Med,
];

impl RowPredictor {
    fn from_byte(v: u8) -> Result<Self> {
        ROW_PREDICTORS
            .get(v as usize)
            .copied()
            .with_context(|| format!("invalid row predictor {v}"))
    }
}

/// Number of rows of a plane; an empty-width image has none.
fn rows_of(len: usize, width: usize) -> usize {
    len.checked_div(width).unwrap_or(0)
}

/// The rule itself, on `sample - base` values of the left, upper and
/// upper-left neighbors.
#[inline(always)]
fn predict_delta(predictor: RowPredictor, left: i32, up: i32, up_left: i32) -> i32 {
    match predictor {
        RowPredictor::None => 0,
        RowPredictor::Left => left,
        RowPredictor::Up => up,
        // Floor division by 2; `>>` is arithmetic on i32.
        RowPredictor::Average => (left + up) >> 1,
        RowPredictor::Paeth => {
            let (pl, pu, pul) = (
                (up - up_left).abs(),
                (left - up_left).abs(),
                (left + up - 2 * up_left).abs(),
            );
            // Written as selects rather than nested branches; same tie order
            // as PNG: left, then up, then upper-left.
            let up_or_up_left = if pu <= pul { up } else { up_left };
            if pl <= pu && pl <= pul {
                left
            } else {
                up_or_up_left
            }
        }
        // Median of left, up and the gradient estimate.
        RowPredictor::Med => (left + up - up_left).clamp(left.min(up), left.max(up)),
    }
}

/// Prediction for sample `i` from already reconstructed samples. With a base
/// (the decoded green plane), the predictor works on `sample - base`. Except
/// for `None`, the first sample predicts 0, the rest of the first row uses
/// the left neighbor and the first column uses the neighbor above.
fn prediction(
    predictor: RowPredictor,
    recon: &[u8],
    base: Option<&[u8]>,
    width: usize,
    i: usize,
) -> i32 {
    let b = |j: usize| base.map_or(0, |b| b[j] as i32);
    let d = |j: usize| recon[j] as i32 - b(j);
    let delta = if predictor == RowPredictor::None || i == 0 {
        0
    } else if i < width {
        d(i - 1)
    } else if i.is_multiple_of(width) {
        d(i - width)
    } else {
        predict_delta(predictor, d(i - 1), d(i - width), d(i - width - 1))
    };
    (b(i) + delta).clamp(0, 255)
}

fn native_stream(
    width: usize,
    rgba: &[u8],
    palette: Option<&Palette>,
    transform: Transform,
    predictor: Predictor,
    max_error: u8,
) -> Result<Vec<u8>> {
    let q = Quantizer::new(max_error);
    let mut out = vec![transform as u8, predictor as u8, max_error];
    if transform == Transform::Palette {
        let palette = palette.context("palette transform without a palette")?;
        ensure!(max_error == 0, "palette encoding is lossless only");
        out.push((palette.colors.len() - 1) as u8);
        out.extend(palette.colors.iter().flatten());
        write_channel(&mut out, &palette.indices, None, width, predictor, q)?;
        return Ok(out);
    }
    let mut green = None;
    for c in CHANNEL_ORDER {
        let samples: Vec<u8> = rgba.iter().skip(c).step_by(4).copied().collect();
        let base = if transform == Transform::SubtractGreen && (c == 0 || c == 2) {
            green.as_deref()
        } else {
            None
        };
        let recon = write_channel(&mut out, &samples, base, width, predictor, q)?;
        if c == 1 {
            green = Some(recon);
        }
    }
    Ok(out)
}

/// Codes row `y` with `predictor`, writing reconstructions into `recon` and
/// symbols into `symbols`. Returns the sum of the signed symbol magnitudes,
/// the encoder's estimate of the row's cost.
#[allow(clippy::too_many_arguments)]
fn code_row(
    samples: &[u8],
    recon: &mut [u8],
    symbols: &mut [u8],
    base: Option<&[u8]>,
    width: usize,
    y: usize,
    predictor: RowPredictor,
    q: Quantizer,
) -> u64 {
    let mut cost = 0;
    for i in y * width..(y + 1) * width {
        let p = prediction(predictor, recon, base, width, i);
        let s = q.symbol(samples[i], p);
        recon[i] = q.reconstruct(p, s);
        symbols[i] = s;
        cost += q.signed(s).unsigned_abs() as u64;
    }
    cost
}

/// Writes one channel and returns its reconstruction, which is what the
/// decoder will see and therefore what later channels must predict from.
fn write_channel(
    out: &mut Vec<u8>,
    samples: &[u8],
    base: Option<&[u8]>,
    width: usize,
    predictor: Predictor,
    q: Quantizer,
) -> Result<Vec<u8>> {
    if let [first, rest @ ..] = samples {
        if rest.iter().all(|v| v == first) {
            out.extend([CHANNEL_CONSTANT, *first]);
            return Ok(samples.to_vec());
        }
    }
    out.push(CHANNEL_CODED);
    let rows = rows_of(samples.len(), width);
    let mut recon = vec![0u8; samples.len()];
    let mut symbols = vec![0u8; samples.len()];
    let mut row_predictors = Vec::with_capacity(rows);
    for y in 0..rows {
        let chosen = match predictor {
            Predictor::None => RowPredictor::None,
            Predictor::Up => RowPredictor::Up,
            // Encoding speed is not a priority: try every rule on the row and
            // keep the one with the smallest residuals.
            Predictor::PerRow => ROW_PREDICTORS
                .into_iter()
                .min_by_key(|&rp| {
                    code_row(samples, &mut recon, &mut symbols, base, width, y, rp, q)
                })
                .expect("non-empty predictor list"),
        };
        code_row(samples, &mut recon, &mut symbols, base, width, y, chosen, q);
        row_predictors.push(chosen as u8);
    }
    if predictor == Predictor::PerRow {
        write_plane(out, &row_predictors)?;
    }
    write_plane(out, &symbols)?;
    Ok(recon)
}

fn read_channel(
    data: &[u8],
    at: &mut usize,
    pixels: usize,
    base: Option<&[u8]>,
    width: usize,
    predictor: Predictor,
    q: Quantizer,
) -> Result<Vec<u8>> {
    match take(data, at, 1)?[0] {
        CHANNEL_CONSTANT => Ok(vec![take(data, at, 1)?[0]; pixels]),
        CHANNEL_CODED => {
            let rows = rows_of(pixels, width);
            let row_predictors: Vec<RowPredictor> = match predictor {
                Predictor::None => vec![RowPredictor::None; rows],
                Predictor::Up => vec![RowPredictor::Up; rows],
                Predictor::PerRow => read_plane(data, at, rows)?
                    .into_iter()
                    .map(RowPredictor::from_byte)
                    .collect::<Result<_>>()?,
            };
            // Symbols are replaced by samples in place; prediction only reads
            // positions that are already reconstructed.
            let mut plane = read_plane(data, at, pixels)?;
            for (y, &rp) in row_predictors.iter().enumerate() {
                if q.near == 0 && reconstruct_row_lossless(&mut plane, base, width, y, rp) {
                    continue;
                }
                for i in y * width..(y + 1) * width {
                    let s = plane[i];
                    ensure!((s as i32) < q.range, "symbol outside quantizer range");
                    let p = prediction(rp, &plane, base, width, i);
                    plane[i] = q.reconstruct(p, s);
                }
            }
            Ok(plane)
        }
        kind => bail!("invalid channel kind {kind}"),
    }
}

/// Lossless fast path of the decoder for every row below the first (and for
/// `None` rows anywhere). With `max_error = 0` reconstruction is
/// `sample = prediction + symbol (mod 256)`: `None` and `Up` rows are slice
/// operations the compiler vectorizes, and the other rules use a tight
/// sequential loop. Returns false for rows it does not handle. Must agree exactly with `prediction`; the roundtrip tests compare
/// the two.
fn reconstruct_row_lossless(
    plane: &mut [u8],
    base: Option<&[u8]>,
    width: usize,
    y: usize,
    predictor: RowPredictor,
) -> bool {
    let row = y * width..(y + 1) * width;
    match (predictor, base) {
        (RowPredictor::None, None) => true,
        (RowPredictor::None, Some(base)) => {
            for (x, &b) in plane[row.clone()].iter_mut().zip(&base[row]) {
                *x = x.wrapping_add(b);
            }
            true
        }
        (RowPredictor::Up, None) if y > 0 => {
            let (done, rest) = plane.split_at_mut(y * width);
            let above = &done[(y - 1) * width..];
            for (x, &a) in rest[..width].iter_mut().zip(above) {
                *x = x.wrapping_add(a);
            }
            true
        }
        (RowPredictor::Up, Some(base)) if y > 0 => {
            let (done, rest) = plane.split_at_mut(y * width);
            let above = &done[(y - 1) * width..];
            let above_base = &base[(y - 1) * width..y * width];
            for (((x, &a), &b), &ab) in rest[..width]
                .iter_mut()
                .zip(above)
                .zip(&base[row])
                .zip(above_base)
            {
                let p = (b as i16 + a as i16 - ab as i16).clamp(0, 255) as u8;
                *x = x.wrapping_add(p);
            }
            true
        }
        (_, None) if y > 0 => {
            reconstruct_row_sequential::<false>(plane, &[], width, y, predictor);
            true
        }
        (_, Some(base)) if y > 0 => {
            reconstruct_row_sequential::<true>(plane, base, width, y, predictor);
            true
        }
        _ => false,
    }
}

/// Lossless reconstruction of a row below the first for any rule, keeping the
/// neighbors in locals instead of re-reading them through `prediction`.
fn reconstruct_row_sequential<const HAS_BASE: bool>(
    plane: &mut [u8],
    base: &[u8],
    width: usize,
    y: usize,
    predictor: RowPredictor,
) {
    let (done, rest) = plane.split_at_mut(y * width);
    let above = &done[(y - 1) * width..];
    let row = &mut rest[..width];
    let base_at = |j: usize| if HAS_BASE { base[j] as i32 } else { 0 };
    let (mut left, mut up_left) = (0, 0);
    for x in 0..width {
        let b = base_at(y * width + x);
        let up = above[x] as i32 - base_at((y - 1) * width + x);
        // First column: the neighbor above, as in `prediction`.
        let delta = if x == 0 {
            up
        } else {
            predict_delta(predictor, left, up, up_left)
        };
        let v = row[x].wrapping_add((b + delta).clamp(0, 255) as u8);
        row[x] = v;
        left = v as i32 - b;
        up_left = up;
    }
}

fn decode_native(width: u32, height: u32, plain: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let pixels = width as usize * height as usize;
    let w = width as usize;
    let mut at = 0;
    let header = take(plain, &mut at, 3)?;
    let transform = match header[0] {
        0 => Transform::Direct,
        1 => Transform::SubtractGreen,
        2 => Transform::Palette,
        t => bail!("invalid transform {t}"),
    };
    let predictor = match header[1] {
        0 => Predictor::None,
        1 => Predictor::Up,
        2 => Predictor::PerRow,
        p => bail!("invalid predictor {p}"),
    };
    let q = Quantizer::new(header[2]);
    let mut rgba = vec![0u8; pixels * 4];
    let out = rgba.as_chunks_mut::<4>().0;
    if transform == Transform::Palette {
        ensure!(header[2] == 0, "palette encoding is lossless only");
        let count = take(plain, &mut at, 1)?[0] as usize + 1;
        let colors = take(plain, &mut at, count * 4)?.as_chunks::<4>().0;
        let indices = read_channel(plain, &mut at, pixels, None, w, predictor, q)?;
        for (px, &i) in out.iter_mut().zip(&indices) {
            *px = *colors
                .get(i as usize)
                .context("palette index out of range")?;
        }
    } else {
        let mut green = None;
        for c in CHANNEL_ORDER {
            let base = if transform == Transform::SubtractGreen && (c == 0 || c == 2) {
                green.as_deref()
            } else {
                None
            };
            let plane = read_channel(plain, &mut at, pixels, base, w, predictor, q)?;
            for (px, &v) in out.iter_mut().zip(&plane) {
                px[c] = v;
            }
            if c == 1 && transform == Transform::SubtractGreen {
                green = Some(plane);
            }
        }
    }
    ensure!(at == plain.len(), "trailing native stream bytes");
    Ok((width, height, rgba))
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
mod proptests;

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
    /// Every candidate encoding, not just the one the encoder keeps.
    #[test]
    fn every_candidate_roundtrips() {
        let mut few_colors = Vec::new();
        let mut many_colors = Vec::new();
        for y in 0..23u32 {
            for x in 0..41u32 {
                few_colors.extend([(x / 8 * 40) as u8, (y / 6 * 60) as u8, 30, 255]);
                many_colors.extend([(x * 6) as u8, (y * 11) as u8, (x * y) as u8, (x + y) as u8]);
            }
        }
        for rgba in [few_colors, many_colors] {
            let (palette, list) = candidates(&rgba, 0, false);
            for (transform, predictor, error) in list {
                let plain = native_stream(41, &rgba, palette.as_ref(), transform, predictor, error)
                    .unwrap();
                let decoded = decode_native(41, 23, &plain).unwrap().2;
                assert_eq!(decoded, rgba, "{transform:?}/{predictor:?}");
            }
        }
    }
    /// The predictor byte of a native file's stream.
    pub(crate) fn stored_predictor(file: &[u8]) -> Option<u8> {
        let c = parse_container(file, MAX_PIXELS).unwrap();
        (c.storage == STORAGE_NATIVE)
            .then(|| zstd::bulk::decompress(c.payload, c.plain_len as usize).unwrap()[1])
    }
    #[test]
    fn fast_decode_never_uses_per_row_prediction() {
        // A photo-like image, where per-row prediction normally wins.
        let mut seed = 11u32;
        let p: Vec<u8> = (0..96 * 64u32)
            .flat_map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let n = seed >> 28;
                let (x, y) = (i % 96, i / 96);
                [(x * 2 + n) as u8, (x + y + n) as u8, (y * 3 + n) as u8, 255]
            })
            .collect();
        let default = encode_rgba(96, 64, &p, EncodeOptions::default()).unwrap();
        let options = EncodeOptions {
            fast_decode: true,
            ..Default::default()
        };
        let fast = encode_rgba(96, 64, &p, options).unwrap();
        assert_eq!(stored_predictor(&default), Some(Predictor::PerRow as u8));
        assert!(stored_predictor(&fast).unwrap() < Predictor::PerRow as u8);
        assert_eq!(decode_rgba(&fast).unwrap().2, p);
        assert!(default.len() <= fast.len());
    }
    #[test]
    fn icc_profile_roundtrips_in_both_storages() {
        let icc: Vec<u8> = (0..=255).cycle().take(3000).collect();
        let metadata = Metadata {
            icc_profile: Some(icc.clone()),
        };
        let p = vec![9u8; 8 * 8 * 4];
        let native =
            encode_rgba_with_metadata(8, 8, &p, &metadata, EncodeOptions::default()).unwrap();
        let png = png_bytes(8, 8, &p).unwrap();
        let fallback = container_with_icc(8, 8, STORAGE_PNG, png.len() as u64, Some(&icc), &png);
        for file in [native, fallback] {
            let d = decode_with_metadata(&file, DecodeLimits::default()).unwrap();
            assert_eq!(
                (d.rgba, d.metadata.icc_profile),
                (p.clone(), Some(icc.clone()))
            );
        }
        let plain = encode_rgba(8, 8, &p, EncodeOptions::default()).unwrap();
        let d = decode_with_metadata(&plain, DecodeLimits::default()).unwrap();
        assert_eq!(d.metadata, Metadata::default());
    }
    #[test]
    fn rejects_bad_icc_container_fields() {
        let metadata = Metadata {
            icc_profile: Some(vec![1, 2, 3]),
        };
        let p = vec![9u8; 4];
        let file =
            encode_rgba_with_metadata(1, 1, &p, &metadata, EncodeOptions::default()).unwrap();
        // ICC length pointing past the end of the file.
        let mut long = file.clone();
        long[HEADER_LEN..HEADER_LEN + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_rgba(&long).is_err());
        // Unknown flag bit next to the ICC flag.
        let mut flags = file.clone();
        flags[6] |= 2;
        assert!(decode_rgba(&flags).is_err());
        // Oversized profiles are refused when encoding.
        let huge = Metadata {
            icc_profile: Some(vec![0; MAX_ICC + 1]),
        };
        assert!(encode_rgba_with_metadata(1, 1, &p, &huge, EncodeOptions::default()).is_err());
    }
    /// A noisy photo-like JPEG: much smaller than any lossless encoding.
    fn noisy_jpeg() -> Vec<u8> {
        let mut seed = 5u32;
        let img = image::RgbaImage::from_fn(120, 80, |x, y| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let n = (seed >> 27) as u8;
            image::Rgba([
                (x * 2) as u8 ^ n,
                ((y * 3) as u8).wrapping_add(n),
                (x + y) as u8,
                255,
            ])
        });
        source::tests::jpeg_with_orientation(&img, 1)
    }
    #[test]
    fn keeps_a_smaller_original_file() {
        let jpeg = noisy_jpeg();
        let file = encode_file(&jpeg, EncodeOptions::default()).unwrap();
        assert_eq!(storage(&file).unwrap(), Storage::Original);
        assert_eq!(file.len(), HEADER_LEN + jpeg.len());
        let expected = read_source(&jpeg, DecodeLimits::default()).unwrap();
        let d = decode_with_metadata(&file, DecodeLimits::default()).unwrap();
        assert_eq!(
            (d.width, d.height, d.rgba),
            (expected.width, expected.height, expected.rgba)
        );
        // Disabled: BIC's own encoding even though it is larger.
        let options = EncodeOptions {
            keep_original: false,
            ..Default::default()
        };
        let transcoded = encode_file(&jpeg, options).unwrap();
        assert_eq!(storage(&transcoded).unwrap(), Storage::Native);
        assert!(transcoded.len() > file.len());
    }
    #[test]
    fn transcodes_when_bic_is_smaller() {
        use image::ImageEncoder;
        // An uncompressed-looking PNG of flat graphics.
        let img = image::RgbaImage::from_fn(200, 100, |x, _| {
            image::Rgba([if x < 100 { 10 } else { 200 }, 50, 90, 255])
        });
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut png,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        )
        .write_image(img.as_raw(), 200, 100, image::ExtendedColorType::Rgba8)
        .unwrap();
        let file = encode_file(&png, EncodeOptions::default()).unwrap();
        assert_eq!(storage(&file).unwrap(), Storage::Native);
        assert!(file.len() < png.len());
        assert_eq!(decode_rgba(&file).unwrap().2, img.into_raw());
    }
    #[test]
    fn rejects_unsupported_or_mismatched_embedded_files() {
        let bmp_like = container_with_icc(1, 1, STORAGE_ORIGINAL, 0, None, b"BM\0\0\0\0");
        let err = decode_rgba(&bmp_like).unwrap_err();
        assert!(format!("{err:#}").contains("not PNG, JPEG"), "{err:#}");
        let jpeg = noisy_jpeg();
        let wrong_size = container_with_icc(7, 7, STORAGE_ORIGINAL, 0, None, &jpeg);
        let err = decode_rgba(&wrong_size).unwrap_err();
        assert!(format!("{err:#}").contains("contradict"), "{err:#}");
    }
    #[test]
    fn quantizer_bounds_error_for_every_input() {
        for max_error in [0u8, 1, 2, 3, 4, 7, 10, 31, 64, 127, 200, 255] {
            let q = Quantizer::new(max_error);
            for x in 0..=255u8 {
                for p in 0..=255 {
                    let s = q.symbol(x, p);
                    assert!((s as i32) < q.range);
                    let r = q.reconstruct(p, s);
                    assert!(
                        x.abs_diff(r) <= max_error,
                        "x={x} p={p} near={max_error} -> {r}"
                    );
                }
            }
        }
    }
    #[test]
    fn near_lossless_stays_within_max_error() {
        // Gradient plus small noise, like a photograph: the case near-lossless
        // coding is for.
        let mut seed = 7u32;
        let mut noisy = |v: u32| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (v + (seed >> 29)).min(255) as u8
        };
        let p: Vec<u8> = (0..64 * 64u32)
            .flat_map(|i| [noisy(i % 64 * 3), noisy(i / 64 * 3), noisy(128), 255])
            .collect();
        let lossless = encode_rgba(64, 64, &p, EncodeOptions::default()).unwrap();
        let options = EncodeOptions {
            max_error: 3,
            ..Default::default()
        };
        let lossy = encode_rgba(64, 64, &p, options).unwrap();
        let decoded = decode_rgba(&lossy).unwrap().2;
        assert!(p.iter().zip(&decoded).all(|(a, b)| a.abs_diff(*b) <= 3));
        assert!(lossy.len() < lossless.len());
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
    /// Direct transform, no predictor, lossless, four constant channels.
    const ALL_CONSTANT: [u8; 11] = [0, 0, 0, 0, 1, 0, 2, 0, 3, 0, 4];
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
    /// Plain stream with one coded 1x1 RAW channel holding `symbol`.
    fn single_symbol(header: [u8; 3], extra: &[u8], symbol: u8) -> Vec<u8> {
        let mut plain = header.to_vec();
        plain.extend(extra);
        plain.push(CHANNEL_CODED);
        plain.extend(1u32.to_le_bytes());
        plain.extend(1u16.to_le_bytes());
        plain.push(0);
        plain.extend(1u32.to_le_bytes());
        plain.push(symbol);
        plain
    }
    #[test]
    fn rejects_malformed_native_streams() {
        // Per-row predictor stream whose single row names rule 9.
        let mut bad_row = vec![0, 2, 0, CHANNEL_CODED];
        bad_row.extend(1u32.to_le_bytes());
        bad_row.extend(1u16.to_le_bytes());
        bad_row.push(0);
        bad_row.extend(1u32.to_le_bytes());
        bad_row.push(9);
        let cases: [(&str, Vec<u8>); 6] = [
            ("invalid row predictor", bad_row),
            ("invalid transform", vec![3, 0, 0]),
            ("invalid predictor", vec![0, 3, 0]),
            ("lossless only", vec![2, 0, 1, 0, 1, 2, 3, 4]),
            (
                "palette index out of range",
                single_symbol([2, 0, 0], &[0, 9, 9, 9, 9], 1),
            ),
            (
                "symbol outside quantizer range",
                // max_error 127 leaves a range of 3 symbols.
                single_symbol([0, 0, 127], &[], 3),
            ),
        ];
        for (expected, plain) in cases {
            let err = decode_rgba(&native_file(1, 1, &plain)).unwrap_err();
            assert!(format!("{err:#}").contains(expected), "{expected}: {err:#}");
        }
    }
    #[test]
    fn rejects_payload_inflating_past_declared_length() {
        let bomb = zstd::stream::encode_all(&vec![0u8; 10_000_000][..], 3).unwrap();
        let file = container(1, 1, STORAGE_NATIVE, 64, &bomb);
        assert!(decode_rgba(&file).is_err());
    }
    #[test]
    fn rejects_dimensions_over_memory_limit_before_allocating() {
        let file = native_file(8192, 8192, &ALL_CONSTANT);
        let limits = DecodeLimits {
            max_alloc: 1 << 20,
            ..Default::default()
        };
        let err = decode_rgba_with_limits(&file, limits).unwrap_err();
        assert!(err.to_string().contains("memory limit"), "{err}");
        // Within the default limit the same all-constant stream is valid.
        assert_eq!(
            decode_rgba(&native_file(64, 64, &ALL_CONSTANT))
                .unwrap()
                .2
                .len(),
            64 * 64 * 4
        );
    }
    #[test]
    fn rejects_nonzero_flags() {
        let mut file = native_file(1, 1, &ALL_CONSTANT);
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
