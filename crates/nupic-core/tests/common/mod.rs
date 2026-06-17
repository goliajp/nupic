//! Shared integration-test helpers.
//!
//! These intentionally stay on the public API of `nupic-core` so the tests
//! they support test *contracts*, not implementation details. When the
//! cement-layer impls swap to self-built pipelines, these fixtures keep
//! working as long as the API surface is preserved.

use nupic_core::{Color, Image, MockOpts, MockStyle, Size};

/// Build a deterministic test image of the requested size.
///
/// Uses `mock::Solid` so the pixel content is uniform and predictable.
/// All op tests that need an input image can call this.
pub fn fixture(w: u32, h: u32) -> Image {
    let mut opts = MockOpts::new(Size::new(w, h));
    opts.style = MockStyle::Solid;
    opts.background = Color::rgb(100, 150, 200);
    opts.foreground = Color::rgb(50, 50, 50);
    opts.text = Some(String::new()); // skip label rendering for a clean fixture
    nupic_core::ops::mock::render(opts).expect("fixture render must succeed")
}

/// Build a complex-content fixture that exercises the palette-quantize
/// encoder path. Cycle 60: switched from Gradient to Stripes because
/// the Cycle 25 gradient-detector routes Gradient content to lossless
/// (Auto == Lossless byte-identical → broke the original `Auto <
/// Lossless` assertion). Stripes produces a high-`adj_mn` pattern that
/// bypasses gradient detection and forces the indexed-palette path.
pub fn complex_fixture(w: u32, h: u32) -> Image {
    let mut opts = MockOpts::new(Size::new(w, h));
    opts.style = MockStyle::Stripes;
    opts.background = Color::rgb(20, 50, 200);
    opts.foreground = Color::rgb(220, 180, 40);
    opts.text = Some(String::new());
    nupic_core::ops::mock::render(opts).expect("complex fixture render must succeed")
}
