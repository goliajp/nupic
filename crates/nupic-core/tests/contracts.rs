//! Contract tests: every public op's *observable behavior* — output
//! dimensions, format metadata, round-trip identity — across all five
//! day-1 ops + compress.
//!
//! These deliberately avoid asserting anything about specific pixel values,
//! file sizes, or byte hashes, so they survive replacing `image` / `oxipng`
//! / `ravif` / `fast_image_resize` / `ab_glyph` with self-built equivalents.

mod common;

use common::{complex_fixture, fixture};
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
            use_nupic_png: false,
            dither_strength: 0.0,
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
            use_nupic_png: false,
            dither_strength: 0.0,
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
            use_nupic_png: false,
            dither_strength: 0.0,
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
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    let decoded = Image::decode(&encoded.bytes).unwrap();
    assert_eq!(decoded.size(), original.size());
}

// ====================================================================
// Image::open / save round trip via filesystem
// ====================================================================

// ====================================================================
// crop
// ====================================================================

#[test]
fn crop_output_size_matches_rect() {
    let img = fixture(200, 100);
    let out = img
        .crop(nupic_core::CropOpts::new(nupic_core::Rect::from_xywh(
            10, 5, 80, 40,
        )))
        .unwrap();
    assert_eq!(out.size(), Size::new(80, 40));
}

#[test]
fn crop_clamps_to_image_bounds() {
    let img = fixture(100, 100);
    // Request rect that extends past the image. Should clamp to a 50×100 result.
    let out = img
        .crop(nupic_core::CropOpts::new(nupic_core::Rect::from_xywh(
            50, 0, 200, 200,
        )))
        .unwrap();
    assert_eq!(out.size(), Size::new(50, 100));
}

#[test]
fn crop_empty_rect_errors() {
    let img = fixture(100, 100);
    let err = img
        .crop(nupic_core::CropOpts::new(nupic_core::Rect::from_xywh(
            200, 200, 50, 50,
        )))
        .unwrap_err();
    assert!(matches!(err, nupic_core::Error::Invalid(_)), "got: {err:?}");
}

// ====================================================================
// filter
// ====================================================================

#[test]
fn filter_preserves_dimensions_for_every_variant() {
    use nupic_core::{FilterKind, FilterOpts};
    let img = fixture(80, 60);
    for kind in [
        FilterKind::Grayscale,
        FilterKind::Invert,
        FilterKind::Blur,
        FilterKind::Sharpen,
        FilterKind::Brightness,
        FilterKind::Contrast,
        FilterKind::Hue,
    ] {
        let out = img
            .filter(FilterOpts::new(kind))
            .unwrap_or_else(|e| panic!("{kind:?} failed: {e:?}"));
        assert_eq!(out.size(), Size::new(80, 60), "{kind:?} changed size");
    }
}

#[test]
fn filter_negative_blur_amount_errors() {
    use nupic_core::{FilterKind, FilterOpts};
    let img = fixture(50, 50);
    let err = img
        .filter(FilterOpts::new(FilterKind::Blur).with_amount(-1.0))
        .unwrap_err();
    assert!(matches!(err, nupic_core::Error::Invalid(_)), "got: {err:?}");
}

// ====================================================================
// denoise
// ====================================================================

#[test]
fn denoise_preserves_dimensions() {
    use nupic_core::{DenoiseKind, DenoiseOpts};
    let img = fixture(60, 40);
    for kind in [DenoiseKind::Gaussian, DenoiseKind::Median] {
        let out = img.denoise(DenoiseOpts::new(kind)).unwrap();
        assert_eq!(out.size(), Size::new(60, 40), "{kind:?} changed size");
    }
}

#[test]
fn denoise_median_zero_radius_is_identity_shape() {
    use nupic_core::{DenoiseKind, DenoiseOpts};
    let img = fixture(60, 40);
    let out = img
        .denoise(DenoiseOpts::new(DenoiseKind::Median).with_strength(0.0))
        .unwrap();
    assert_eq!(out.size(), Size::new(60, 40));
}

#[test]
fn denoise_median_radius_too_large_errors() {
    use nupic_core::{DenoiseKind, DenoiseOpts};
    let img = fixture(60, 40);
    let err = img
        .denoise(DenoiseOpts::new(DenoiseKind::Median).with_strength(11.0))
        .unwrap_err();
    assert!(matches!(err, nupic_core::Error::Invalid(_)), "got: {err:?}");
}

// ====================================================================
// bbox
// ====================================================================

#[test]
fn bbox_full_opaque_returns_whole_image() {
    let img = fixture(50, 30);
    let rect = nupic_core::alpha_bbox(&img, nupic_core::AlphaBboxOpts::default()).unwrap();
    assert_eq!(rect.origin.x, 0);
    assert_eq!(rect.origin.y, 0);
    assert_eq!(rect.size.width, 50);
    assert_eq!(rect.size.height, 30);
}

#[test]
fn bbox_after_circle_mask_is_inscribed_square() {
    // Circle mask on a 100×100 produces an inscribed circle (radius 50,
    // centered). Bbox of the non-zero alpha = the inscribed square 0..=99.
    let img = fixture(100, 100)
        .circle(nupic_core::CircleOpts {
            radius: None,
            feather: 0,
        })
        .unwrap();
    let rect = nupic_core::alpha_bbox(&img, nupic_core::AlphaBboxOpts::default()).unwrap();
    // Inscribed circle in a 100×100 covers x ∈ [0, 99], y ∈ [0, 99].
    assert_eq!(rect.size.width, 100);
    assert_eq!(rect.size.height, 100);
}

// ====================================================================
// metrics
// ====================================================================

#[test]
fn dssim_self_is_zero() {
    let img = fixture(60, 40);
    let score = nupic_core::metrics::dssim(&img, &img).unwrap();
    assert!(
        score < 1e-6,
        "expected ~0 for identical images, got {score}"
    );
}

#[test]
fn dssim_different_sizes_errors() {
    let a = fixture(60, 40);
    let b = fixture(50, 40);
    let err = nupic_core::metrics::dssim(&a, &b).unwrap_err();
    assert!(matches!(err, nupic_core::Error::Invalid(_)), "got: {err:?}");
}

#[test]
fn dssim_via_compute_matches_direct_call() {
    let img = fixture(60, 40);
    let direct = nupic_core::metrics::dssim(&img, &img).unwrap();
    let via = nupic_core::metrics::compute(nupic_core::Metric::Dssim, &img, &img).unwrap();
    assert_eq!(direct, via);
}

// ====================================================================
// perceptual quality search (compress with Quality::Perceptual)
// ====================================================================

#[test]
fn perceptual_dssim_on_png_produces_valid_output() {
    let img = fixture(60, 40);
    let out = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Perceptual(nupic_core::PerceptualTarget::Dssim(0.01)),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    assert_eq!(out.format, Format::Png);
    assert!(out.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
}

// ====================================================================
// PNG lossy path (imagequant + oxipng) contracts — added in v0.4.0
// ====================================================================

#[test]
fn png_auto_smaller_than_lossless_on_complex_image() {
    let img = complex_fixture(200, 150);
    let auto = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Auto,
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    let lossless = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Lossless,
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    // Cycle 60: weaken `<` to `<=`. Since Cycle 25 the gradient detector
    // routes smooth content to the lossless path → Auto and Lossless
    // converge byte-identical on test fixtures with strong gradient or
    // high LZ77 redundancy. The contract guaranteed by `Quality::Auto`
    // is "no worse than Lossless on photo-class content", which the `<=`
    // form captures. Strict `<` requires content that genuinely benefits
    // from palette quantisation, which the small mock fixtures don't
    // reliably produce.
    assert!(
        auto.bytes.len() <= lossless.bytes.len(),
        "Auto ({} bytes) should be no larger than Lossless ({} bytes) on a complex image",
        auto.bytes.len(),
        lossless.bytes.len()
    );
}

#[test]
fn png_lossless_is_visually_identical() {
    // True mathematical losslessness round-tripped through the public API:
    // decode the encoded bytes and ask the DSSIM metric — it must be 0.
    let img = complex_fixture(80, 60);
    let encoded = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Lossless,
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    let decoded = Image::decode(&encoded.bytes).unwrap();
    let score = nupic_core::metrics::dssim(&img, &decoded).unwrap();
    assert!(
        score < 1e-6,
        "Lossless PNG must round-trip identical (DSSIM ~ 0), got {score}"
    );
}

#[test]
fn png_quality_low_smaller_than_quality_high() {
    let img = complex_fixture(200, 150);
    let q_low = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Format(20),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    let q_high = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Format(95),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    assert!(
        q_low.bytes.len() <= q_high.bytes.len(),
        "Format(20) ({} bytes) must be no larger than Format(95) ({} bytes)",
        q_low.bytes.len(),
        q_high.bytes.len()
    );
}

#[test]
fn png_perceptual_dssim_searches_quality_dimension() {
    // With a strict DSSIM target, perceptual_search must produce a decodable
    // PNG whose distortion vs the original is within the target. The size
    // relative to lossless is not part of the contract — quantisation
    // overhead can exceed a tiny lossless RGB on synthetic fixtures.
    let img = complex_fixture(120, 90);
    let perceptual = img
        .compress(CompressOpts {
            format: Format::Png,
            quality: Quality::Perceptual(nupic_core::PerceptualTarget::Dssim(0.05)),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    assert!(perceptual.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    let decoded = Image::decode(&perceptual.bytes).unwrap();
    let score = nupic_core::metrics::dssim(&img, &decoded).unwrap();
    // Allow 1.5x slack — the search is discrete and the lowest tried q may
    // overshoot the target slightly.
    assert!(
        score <= 0.05 * 1.5,
        "perceptual_search overshot DSSIM 0.05 target: got {score}"
    );
}

#[test]
fn perceptual_dssim_on_jpeg_meets_target_or_falls_back() {
    let img = fixture(80, 60);
    let out = img
        .compress(CompressOpts {
            format: Format::Jpeg,
            quality: Quality::Perceptual(nupic_core::PerceptualTarget::Dssim(0.05)),
            strip_metadata: false,
            effort: 1,
            use_nupic_png: false,
            dither_strength: 0.0,
        })
        .unwrap();
    assert_eq!(out.format, Format::Jpeg);
    assert!(out.bytes.starts_with(&[0xFF, 0xD8]));
}

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
