//! Contract tests: every public op's *observable behavior* — output
//! dimensions, format metadata, round-trip identity — across all five
//! day-1 ops + compress.
//!
//! These deliberately avoid asserting anything about specific pixel values,
//! file sizes, or byte hashes, so they survive replacing `image` / `oxipng`
//! / `ravif` / `fast_image_resize` / `ab_glyph` with self-built equivalents.

mod common;

use common::fixture;
use nupic_core::{
    CircleOpts, CompressOpts, FitMode, FitOpts, Format, Image, Quality, ResizeMode, ResizeOpts,
    Size,
};

// ====================================================================
// resize
// ====================================================================

#[test]
fn resize_width_only_preserves_aspect_ratio() {
    let img = fixture(200, 100);
    let out = img
        .resize(ResizeOpts::new(ResizeMode::Width(100)))
        .unwrap();
    assert_eq!(out.size(), Size::new(100, 50));
}

#[test]
fn resize_height_only_preserves_aspect_ratio() {
    let img = fixture(200, 100);
    let out = img
        .resize(ResizeOpts::new(ResizeMode::Height(50)))
        .unwrap();
    assert_eq!(out.size(), Size::new(100, 50));
}

#[test]
fn resize_exact_produces_exact_dimensions() {
    let img = fixture(200, 100);
    let out = img
        .resize(ResizeOpts::new(ResizeMode::Exact {
            width: 80,
            height: 80,
        }))
        .unwrap();
    assert_eq!(out.size(), Size::new(80, 80));
}

#[test]
fn resize_scale_factor_scales_both_dims() {
    let img = fixture(200, 100);
    let out = img.resize(ResizeOpts::new(ResizeMode::Scale(0.5))).unwrap();
    assert_eq!(out.size(), Size::new(100, 50));
}

#[test]
fn resize_to_one_pixel_does_not_panic() {
    let img = fixture(100, 100);
    let out = img
        .resize(ResizeOpts::new(ResizeMode::Exact {
            width: 1,
            height: 1,
        }))
        .unwrap();
    assert_eq!(out.size(), Size::new(1, 1));
}

#[test]
fn resize_to_larger_dimensions_works() {
    let img = fixture(50, 50);
    let out = img
        .resize(ResizeOpts::new(ResizeMode::Exact {
            width: 500,
            height: 500,
        }))
        .unwrap();
    assert_eq!(out.size(), Size::new(500, 500));
}

// ====================================================================
// fit — five modes
// ====================================================================

#[test]
fn fit_contain_outputs_exact_box_size() {
    let img = fixture(400, 200);
    let out = img
        .fit(FitOpts::new(Size::new(100, 100), FitMode::Contain))
        .unwrap();
    assert_eq!(out.size(), Size::new(100, 100));
}

#[test]
fn fit_cover_outputs_exact_box_size() {
    let img = fixture(400, 200);
    let out = img
        .fit(FitOpts::new(Size::new(100, 100), FitMode::Cover))
        .unwrap();
    assert_eq!(out.size(), Size::new(100, 100));
}

#[test]
fn fit_fill_stretches_to_exact_box_size() {
    let img = fixture(400, 200);
    let out = img
        .fit(FitOpts::new(Size::new(100, 100), FitMode::Fill))
        .unwrap();
    assert_eq!(out.size(), Size::new(100, 100));
}

#[test]
fn fit_inside_does_not_upscale() {
    // Image is smaller than the box → keep original dimensions.
    let img = fixture(40, 40);
    let out = img
        .fit(FitOpts::new(Size::new(200, 200), FitMode::Inside))
        .unwrap();
    assert_eq!(out.size(), Size::new(40, 40));
}

#[test]
fn fit_inside_downscales_when_image_exceeds_box() {
    let img = fixture(400, 200);
    let out = img
        .fit(FitOpts::new(Size::new(100, 100), FitMode::Inside))
        .unwrap();
    // Aspect-preserved: contain into 100×100 box.
    assert!(out.width() <= 100 && out.height() <= 100);
    assert!(out.width() == 100 || out.height() == 100);
}

#[test]
fn fit_outside_does_not_downscale() {
    let img = fixture(400, 400);
    let out = img
        .fit(FitOpts::new(Size::new(100, 100), FitMode::Outside))
        .unwrap();
    assert_eq!(out.size(), Size::new(400, 400));
}

#[test]
fn fit_outside_upscales_when_image_smaller_than_box() {
    let img = fixture(40, 40);
    let out = img
        .fit(FitOpts::new(Size::new(200, 200), FitMode::Outside))
        .unwrap();
    assert!(out.width() >= 200 && out.height() >= 200);
}

// ====================================================================
// circle
// ====================================================================

#[test]
fn circle_preserves_input_dimensions() {
    let img = fixture(80, 60);
    let out = img.circle(CircleOpts::default()).unwrap();
    assert_eq!(out.size(), Size::new(80, 60));
}

#[test]
fn circle_with_explicit_radius_preserves_dimensions() {
    let img = fixture(80, 60);
    let out = img
        .circle(CircleOpts {
            radius: Some(15),
            feather: 0,
        })
        .unwrap();
    assert_eq!(out.size(), Size::new(80, 60));
}

#[test]
fn circle_with_feather_preserves_dimensions() {
    let img = fixture(80, 60);
    let out = img
        .circle(CircleOpts {
            radius: None,
            feather: 8,
        })
        .unwrap();
    assert_eq!(out.size(), Size::new(80, 60));
}

// ====================================================================
// compress — output metadata contract
// ====================================================================

#[test]
fn compress_returns_encoded_size_matching_input() {
    let img = fixture(50, 50);
    let encoded = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Lossless,
            strip_metadata: false,
            effort: 1,
        })
        .unwrap();
    assert_eq!(encoded.size, Size::new(50, 50));
    assert_eq!(encoded.format, Format::Png);
    assert!(!encoded.bytes.is_empty());
}

// ====================================================================
// encode → decode round trip
// ====================================================================

#[test]
fn png_encode_decode_round_trip_keeps_dimensions() {
    let original = fixture(123, 47);
    let encoded = original
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Lossless,
            strip_metadata: false,
            effort: 1,
        })
        .unwrap();
    let decoded = Image::decode(&encoded.bytes).unwrap();
    assert_eq!(decoded.size(), original.size());
}

#[test]
fn jpeg_encode_decode_round_trip_keeps_dimensions() {
    let original = fixture(80, 60);
    let encoded = original
        .compress(CompressOpts {
            format: Format::Jpeg,
            quality: Quality::Format(70),
            strip_metadata: false,
            effort: 1,
        })
        .unwrap();
    let decoded = Image::decode(&encoded.bytes).unwrap();
    assert_eq!(decoded.size(), original.size());
}

#[test]
fn webp_lossless_encode_decode_round_trip_keeps_dimensions() {
    let original = fixture(40, 30);
    let encoded = original
        .compress(CompressOpts {
            format: Format::Webp,
            quality: Quality::Lossless,
            strip_metadata: false,
            effort: 1,
        })
        .unwrap();
    let decoded = Image::decode(&encoded.bytes).unwrap();
    assert_eq!(decoded.size(), original.size());
}

// ====================================================================
// Image::open / save round trip via filesystem
// ====================================================================

#[test]
fn open_save_round_trip_through_filesystem() {
    let original = fixture(64, 48);
    let tmp = std::env::temp_dir().join(format!(
        "nupic_test_open_save_{}.png",
        std::process::id()
    ));
    original.save(&tmp).unwrap();
    let reopened = Image::open(&tmp).unwrap();
    assert_eq!(reopened.size(), original.size());
    let _ = std::fs::remove_file(&tmp);
}
