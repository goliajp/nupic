//! `nupic-png` — self-built PNG encoder, indexed-color path.
//!
//! Stage 2 of the [PNG codec roadmap](../../docs/roadmap.md): replaces
//! the `oxipng` dep in `nupic-quantize → Quality::Auto` by writing
//! IHDR + PLTE + (optional) tRNS + IDAT + IEND directly, with per-row
//! filter try-all (None / Sub / Up / Average / Paeth, picked by
//! minimum sum-of-absolute-differences) and IDAT compressed via
//! `nupic-deflate Level::Best` (phase 1.4 iterative cost-DP).
//!
//! Bit-exact round-trip through the `image` crate's PNG decoder is
//! validated in the test suite.
//!
//! # Example
//!
//! ```
//! use nupic_png::{encode_indexed_png, IndexedImage};
//! use rgb::Rgb;
//! let img = IndexedImage {
//!     width: 2,
//!     height: 2,
//!     palette: vec![Rgb { r: 0, g: 0, b: 0 }, Rgb { r: 255, g: 255, b: 255 }],
//!     indices: vec![0, 1, 1, 0],
//!     trns: None,
//! };
//! let png_bytes = encode_indexed_png(&img);
//! assert_eq!(&png_bytes[0..8], b"\x89PNG\r\n\x1a\n");
//! ```

#![allow(clippy::module_name_repetitions)]

use nupic_bits::crc32;
use nupic_deflate::{Level, deflate_level};
use rgb::Rgb;

/// RFC 1950 zlib wrapper around `nupic-deflate` output. Mirrors
/// `nupic_deflate::zlib_compress` but routes through `deflate_level`
/// with our chosen level so we can fall back to Fast on
/// highly-compressible inputs without paying Level::Best's iterative
/// cost-DP overhead. zlib header / Adler-32 footer match RFC 1950.
fn zlib_wrap(data: &[u8], level: Level) -> Vec<u8> {
    use nupic_bits::adler32_update;
    const CMF: u8 = 0x78;
    let cmf = CMF as u16;
    let mut flg: u16 = 0x01;
    let header = cmf * 256 + flg;
    if header % 31 != 0 {
        flg |= (31 - (header % 31)) % 31;
    }
    let deflated = deflate_level(data, level);
    let adler = adler32_update(data, 1);
    let mut out = Vec::with_capacity(deflated.len() + 6);
    out.push(CMF);
    out.push(flg as u8);
    out.extend_from_slice(&deflated);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

mod filter;

pub use filter::{FilterType, filter_image, filter_image_single, filter_image_deflate_aware, filter_image_best_of, mean_run_length};

/// PNG filter-selection strategy.
///
/// - `MinSad` — Heckbert's sum-of-abs heuristic, per-row.
/// - `DeflateAware` — per-row trial-deflate (5 filters × deflate, pick
///   smallest by isolated row size).
/// - `BestOf` — phase 2.2 default. Tries 7 candidates (5 single-filter
///   + per-row min-SAD + per-row deflate-aware), measures each by
///   full-stream `Level::Fast` deflate, picks smallest. Captures
///   cross-row LZ77 context the per-row strategies miss.
#[derive(Clone, Copy, Debug, Default)]
pub enum FilterStrategy {
    MinSad,
    DeflateAware,
    #[default]
    BestOf,
}

/// An 8-bit indexed-color image: palette of up to 256 sRGB colors plus
/// a flat row-major index buffer. Optional `trns` gives per-palette-
/// entry alpha (0 = fully transparent, 255 = fully opaque); when
/// `None`, no `tRNS` chunk is emitted.
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    pub palette: Vec<Rgb<u8>>,
    /// Row-major. `len() == width * height`.
    pub indices: Vec<u8>,
    /// Per-palette alpha. If `Some`, `len() == palette.len()`.
    pub trns: Option<Vec<u8>>,
}

/// Encode an [`IndexedImage`] as a PNG byte stream using the default
/// (`FilterStrategy::MinSad`) filter selection.
#[must_use]
pub fn encode_indexed_png(img: &IndexedImage) -> Vec<u8> {
    encode_indexed_png_with(img, FilterStrategy::default())
}

/// Encode an [`IndexedImage`] as a PNG byte stream with an explicit
/// filter strategy.
#[must_use]
pub fn encode_indexed_png_with(img: &IndexedImage, strategy: FilterStrategy) -> Vec<u8> {
    debug_assert_eq!(
        img.indices.len(),
        (img.width as usize) * (img.height as usize),
        "indices buffer size mismatch"
    );
    debug_assert!(img.palette.len() <= 256, "palette > 256 entries");
    if let Some(trns) = &img.trns {
        debug_assert_eq!(trns.len(), img.palette.len(), "tRNS / palette length mismatch");
    }

    let mut out = Vec::with_capacity(8 + img.indices.len() / 2 + 1024);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    // IHDR — 13 bytes:
    //   width (4) + height (4) + bit_depth(1) + color_type(1)
    //   + compression(1=deflate) + filter(1=adaptive) + interlace(1=none)
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&img.width.to_be_bytes());
    ihdr.extend_from_slice(&img.height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(3); // color type = indexed
    ihdr.push(0); // compression = deflate
    ihdr.push(0); // filter = adaptive
    ihdr.push(0); // interlace = none
    write_chunk(&mut out, b"IHDR", &ihdr);

    // PLTE — 3 × palette.len() bytes
    let mut plte = Vec::with_capacity(img.palette.len() * 3);
    for c in &img.palette {
        plte.push(c.r);
        plte.push(c.g);
        plte.push(c.b);
    }
    write_chunk(&mut out, b"PLTE", &plte);

    // tRNS — optional
    if let Some(trns) = &img.trns {
        write_chunk(&mut out, b"tRNS", trns);
    }

    // IDAT — filter every row, concat, zlib-compress.
    let raw_filtered = match strategy {
        FilterStrategy::MinSad => filter::filter_image(img.width, img.height, &img.indices),
        FilterStrategy::DeflateAware => {
            filter::filter_image_deflate_aware(img.width, img.height, &img.indices)
        }
        FilterStrategy::BestOf => filter::filter_image_best_of(img.width, img.height, &img.indices),
    };
    // Cycle 6 Pass 5 reversion: Level::Best on UI screenshots is 30-150s
    // wall-clock for marginal size win on photos (chain=2048 + iter=10
    // experiment tested no improvement beyond chain=512 + iter=5).
    // Restore 2.4-era size-aware adaptive — Fast on big_and_flat or
    // very_flat keeps wall-clock < 11s while photos stay on Best.
    // 03k essay §5 documents the chain-depth / iter-count sensitivity
    // research showing 2.4-era trade-off curve is empirically optimal.
    let mrl = filter::mean_run_length(&raw_filtered);
    let big_and_flat = raw_filtered.len() > 500_000 && mrl >= 8.0;
    let very_flat = mrl >= 32.0;
    let level = if big_and_flat || very_flat {
        Level::Fast
    } else {
        Level::Best
    };
    let idat = zlib_wrap(&raw_filtered, level);
    write_chunk(&mut out, b"IDAT", &idat);

    // IEND — empty payload
    write_chunk(&mut out, b"IEND", &[]);

    out
}

/// Write a single PNG chunk: 4-byte length + 4-byte type + data + 4-byte CRC.
/// CRC is computed over (type + data).
fn write_chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(ty);
    crc_input.extend_from_slice(data);
    let crc = crc32(&crc_input);
    out.extend_from_slice(&crc.to_be_bytes());
}
