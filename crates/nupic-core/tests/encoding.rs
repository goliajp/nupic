//! Encoded-byte contract tests. Asserts the **format-spec-level magic**
//! at the start of each encoder's output — not byte counts, not hashes.
//!
//! These survive every implementation change because PNG, JPEG, WebP, and
//! AVIF all mandate fixed-byte signatures by spec.

mod common;

use common::fixture;
use nupic_core::{CompressOpts, Format, Quality};

fn encode(format: Format, quality: Quality) -> Vec<u8> {
    let img = fixture(40, 30);
    img.compress(CompressOpts {
        format,
        quality,
        strip_metadata: false,
        effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
    })
    .expect("encode should succeed")
    .bytes
}

#[test]
fn png_output_starts_with_png_signature() {
    let bytes = encode(Format::Png, Quality::Lossless);
    // PNG spec §3.1: signature is 8 bytes 89 50 4E 47 0D 0A 1A 0A.
    assert!(bytes.len() >= 8, "output too short: {}", bytes.len());
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "PNG signature mismatch"
    );
}

#[test]
fn jpeg_output_starts_with_soi_marker() {
    let bytes = encode(Format::Jpeg, Quality::Format(70));
    // JPEG (ITU-T T.81) SOI marker = FF D8.
    assert!(bytes.len() >= 2);
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG SOI mismatch");
    // EOI marker = FF D9 at file end.
    assert_eq!(
        &bytes[bytes.len() - 2..],
        &[0xFF, 0xD9],
        "JPEG EOI mismatch"
    );
}

#[test]
fn webp_lossless_output_has_riff_webp_container() {
    let bytes = encode(Format::Webp, Quality::Lossless);
    // RIFF container: "RIFF" + 4-byte size + "WEBP".
    assert!(bytes.len() >= 12);
    assert_eq!(&bytes[..4], b"RIFF", "WebP RIFF chunk missing");
    assert_eq!(&bytes[8..12], b"WEBP", "WebP form-type missing");
}

#[test]
fn webp_lossy_output_has_riff_webp_container() {
    let bytes = encode(Format::Webp, Quality::Format(70));
    assert!(bytes.len() >= 12);
    assert_eq!(&bytes[..4], b"RIFF", "WebP RIFF chunk missing");
    assert_eq!(&bytes[8..12], b"WEBP", "WebP form-type missing");
}

#[test]
fn gif_output_starts_with_gif_signature() {
    let bytes = encode(Format::Gif, Quality::Format(80));
    // GIF spec §17: signature is `GIF87a` or `GIF89a`.
    assert!(bytes.len() >= 6);
    let sig = &bytes[..6];
    assert!(
        sig == b"GIF87a" || sig == b"GIF89a",
        "GIF signature mismatch: {sig:?}"
    );
}

#[test]
fn bmp_output_starts_with_bm_signature() {
    let bytes = encode(Format::Bmp, Quality::Lossless);
    // BMP file header: bytes 0-1 = "BM".
    assert!(bytes.len() >= 2);
    assert_eq!(&bytes[..2], b"BM", "BMP signature mismatch");
}

#[test]
fn tiff_output_has_byte_order_mark() {
    let bytes = encode(Format::Tiff, Quality::Lossless);
    // TIFF 6.0 §1: bytes 0-1 = "II" (little-endian) or "MM" (big-endian),
    // followed by magic number 42 (in chosen endianness).
    assert!(bytes.len() >= 4);
    let mark = &bytes[..2];
    assert!(
        mark == b"II" || mark == b"MM",
        "TIFF byte order mark mismatch: {mark:?}"
    );
}

#[test]
fn avif_output_has_ftyp_box_with_avif_brand() {
    let bytes = encode(Format::Avif, Quality::Format(40));
    // ISOBMFF: box at offset 0 is ftyp. Box-type at offset 4 == "ftyp"
    // (4 bytes), major brand at offset 8 == "avif" or "avis".
    assert!(bytes.len() >= 12);
    assert_eq!(&bytes[4..8], b"ftyp", "AVIF ftyp box missing");
    let major_brand = &bytes[8..12];
    assert!(
        major_brand == b"avif" || major_brand == b"avis",
        "AVIF major brand expected 'avif'/'avis', got {major_brand:?}"
    );
}
