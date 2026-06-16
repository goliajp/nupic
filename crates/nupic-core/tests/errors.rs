//! Error-variant contract tests. Each test asserts the *type* of error
//! returned for a defined failure mode, not the error message text.
//!
//! Two tests (Perceptual, Lossy WebP) are tagged `[v0.x cleanup]` because
//! they'll need updating when those features land. Until then they document
//! the current "not yet" surface; their failure is a correct signal that
//! we shipped new capability.

mod common;

use common::fixture;
use nupic_core::{
    CompressOpts, Error, Format, MockOpts, PerceptualTarget, Quality, ResizeMode, ResizeOpts,
    Size,
};

// ===== compress error paths =====

#[test]
fn compress_with_auto_format_errors() {
    let img = fixture(50, 50);
    let err = img
        .compress(CompressOpts {
            format: Format::Auto,
            quality: Quality::Format(80),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

#[test]
fn compress_jpeg_lossless_errors() {
    let img = fixture(50, 50);
    let err = img
        .compress(CompressOpts {
            format: Format::Jpeg,
            quality: Quality::Lossless,
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

#[test]
fn compress_jxl_unsupported() {
    let img = fixture(50, 50);
    let err = img
        .compress(CompressOpts {
            format: Format::Jxl,
            quality: Quality::Format(80),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap_err();
    assert!(
        matches!(err, Error::UnsupportedFormat(Format::Jxl)),
        "got: {err:?}"
    );
}

#[test]
fn compress_perceptual_ssimulacra2_works_in_v0_5() {
    // 0.5.0: nupic-ssimulacra stone crate landed → SSIMULACRA2 metric
    // is wired through perceptual_search. Confirm the JPEG path returns
    // valid bytes (no NotImplemented).
    let img = fixture(50, 50);
    let encoded = img
        .compress(CompressOpts {
            format: Format::Jpeg,
            quality: Quality::Perceptual(PerceptualTarget::Ssimulacra2(85.0)),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .expect("Ssimulacra2 perceptual target should work in 0.5.0+");
    assert_eq!(encoded.format, Format::Jpeg);
    assert!(encoded.bytes.starts_with(&[0xFF, 0xD8]),
        "JPEG SOI missing — bytes {:?}", &encoded.bytes[..encoded.bytes.len().min(4)]);
}

#[test]
fn compress_perceptual_butteraugli_still_not_implemented() {
    // [v0.x cleanup] Delete when Butteraugli lands.
    let img = fixture(50, 50);
    let err = img
        .compress(CompressOpts {
            format: Format::Jpeg,
            quality: Quality::Perceptual(PerceptualTarget::Butteraugli(1.0)),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap_err();
    assert!(matches!(err, Error::NotImplemented(_)), "got: {err:?}");
}

// ===== resize error paths =====

#[test]
fn resize_zero_scale_rejected() {
    let img = fixture(100, 100);
    let err = img
        .resize(ResizeOpts::new(ResizeMode::Scale(0.0)))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

#[test]
fn resize_negative_scale_rejected() {
    let img = fixture(100, 100);
    let err = img
        .resize(ResizeOpts::new(ResizeMode::Scale(-2.0)))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

#[test]
fn resize_nan_scale_rejected() {
    let img = fixture(100, 100);
    let err = img
        .resize(ResizeOpts::new(ResizeMode::Scale(f32::NAN)))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

// ===== mock error paths =====

#[test]
fn mock_zero_width_rejected() {
    let err = nupic_core::ops::mock::render(MockOpts::new(Size::new(0, 100))).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

#[test]
fn mock_zero_height_rejected() {
    let err = nupic_core::ops::mock::render(MockOpts::new(Size::new(100, 0))).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "got: {err:?}");
}

// ===== decode error paths =====

#[test]
fn decode_garbage_bytes_errors() {
    let err = nupic_core::Image::decode(&[1, 2, 3, 4, 5]).unwrap_err();
    assert!(matches!(err, Error::Codec(_)), "got: {err:?}");
}

#[test]
fn open_nonexistent_path_errors() {
    let err = nupic_core::Image::open("/definitely/does/not/exist/at/this/path.png").unwrap_err();
    // Could surface as Io or Codec depending on whether `image` opens first;
    // either is a legitimate "could not load" signal.
    assert!(
        matches!(err, Error::Io(_) | Error::Codec(_)),
        "got: {err:?}"
    );
}
