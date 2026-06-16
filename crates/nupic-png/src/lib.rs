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
use nupic_deflate::zlib_compress;
use rgb::Rgb;

mod filter;

pub use filter::FilterType;

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
    let idat = zlib_compress(&raw_filtered);
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
