//! Stone B baseline reimplementation (`B1` phase).
//!
//! Reproduces the SSIMULACRA2 algorithm step-for-step from
//! `cloudinary/ssimulacra2` (Sneyers 2022-2023) / `ssimulacra2`
//! v0.5.1 (rust-av port). Applies the Stone A codegen recipe to every
//! hot kernel:
//!
//! - `f32::mul_add` on every matmul / accumulation
//! - `#[inline(always)]` on per-pixel inner kernels
//! - struct-pass-by-value for tiny RGB / XYB triples where applicable
//!
//! Does **not** SIMD vectorize yet — that's phase B2/B3 in
//! `docs/research/png/03b-ter-*.md`. B1's goal: match cement crate
//! score within 0.5 points + within 10% wall-clock.
//!
//! Color-space conversion goes through `yuvxyb` v0.4.2 (same transitive
//! used by the cement crate). Reimplementing yuvxyb is out of scope for
//! Stone B; we focus on the per-scale pyramid + blur + ssim/edge maps +
//! aggregation that's specific to SSIMULACRA2.

use yuvxyb::{ColorPrimaries, LinearRgb, Rgb, TransferCharacteristic, Xyb};

const NUM_SCALES: usize = 6;
const C2: f32 = 0.0009;

// --- public surface ---------------------------------------------------

/// Compute SSIMULACRA2 score given two sRGB f32 buffers (per pixel).
/// Inputs must be the same width × height, ≥ 8 × 8.
///
/// B1 entry point — single-column vertical IIR scan. Slower on big
/// images; bit-exact match with cement crate v0.5.1.
pub fn ssimulacra2_score_srgb(
    reference: &[[f32; 3]],
    distorted: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<f64, &'static str> {
    score_inner(reference, distorted, width, height, VerticalKind::Single)
}

/// Compute SSIMULACRA2 score with the B2 chunked vertical pass.
/// Same algorithm as `ssimulacra2_score_srgb`; vertical IIR is run
/// 128 → 32 → 1 columns at a time (mirrors cement
/// `vertical_pass_chunked::<128, 32>`) so the strided reads coalesce
/// into L1-friendly windows.
pub fn ssimulacra2_score_srgb_chunked(
    reference: &[[f32; 3]],
    distorted: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<f64, &'static str> {
    score_inner(reference, distorted, width, height, VerticalKind::Chunked)
}

/// Compute SSIMULACRA2 score with the B5 per-scale parallel blurs.
/// On top of B4's row-level parallel horizontal,each scale's 5
/// gaussian_blur calls split into 3 concurrent streams via rayon::join:
///   - σ stream:(mul₁₁ → blur,mul₂₂ → blur,mul₁₂ → blur)— internally sequential
///   - μ₁ stream:gaussian_blur(xyb1)
///   - μ₂ stream:gaussian_blur(xyb2)
pub fn ssimulacra2_score_srgb_b5(
    reference: &[[f32; 3]],
    distorted: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<f64, &'static str> {
    if reference.len() != distorted.len() || reference.len() != width * height {
        return Err("dimension mismatch");
    }
    if width < 8 || height < 8 {
        return Err("image too small");
    }
    let mut img1 = build_linear(reference, width, height)?;
    let mut img2 = build_linear(distorted, width, height)?;

    let mut all_scales: Vec<MsssimScale> = Vec::with_capacity(NUM_SCALES);
    let mut cur_w = width;
    let mut cur_h = height;

    for scale in 0..NUM_SCALES {
        if cur_w < 8 || cur_h < 8 {
            break;
        }
        if scale > 0 {
            img1 = downscale_by_2(&img1);
            img2 = downscale_by_2(&img2);
            cur_w = img1.width();
            cur_h = img1.height();
        }
        all_scales.push(compute_scale_b5(&img1, &img2, cur_w, cur_h));
    }
    Ok(aggregate_score(&all_scales))
}

fn compute_scale_b5(img1: &LinearRgb, img2: &LinearRgb, w: usize, h: usize) -> MsssimScale {
    let xyb1 = make_positive_xyb_planar(img1);
    let xyb2 = make_positive_xyb_planar(img2);

    let xyb1_ref = &xyb1;
    let xyb2_ref = &xyb2;

    // 3 parallel streams. σ stream needs its own mul_buf; μ streams
    // just blur xyb directly.
    let ((sigma1_sq, sigma2_sq, sigma12), (mu1, mu2)) = rayon::join(
        || {
            let mut mul_buf = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
            image_multiply(xyb1_ref, xyb1_ref, &mut mul_buf);
            let s1 = gaussian_blur(&mul_buf, w, h, VerticalKind::ParallelH);
            image_multiply(xyb2_ref, xyb2_ref, &mut mul_buf);
            let s2 = gaussian_blur(&mul_buf, w, h, VerticalKind::ParallelH);
            image_multiply(xyb1_ref, xyb2_ref, &mut mul_buf);
            let s12 = gaussian_blur(&mul_buf, w, h, VerticalKind::ParallelH);
            (s1, s2, s12)
        },
        || rayon::join(
            || gaussian_blur(xyb1_ref, w, h, VerticalKind::ParallelH),
            || gaussian_blur(xyb2_ref, w, h, VerticalKind::ParallelH),
        ),
    );

    MsssimScale {
        avg_ssim: ssim_map(w, h, &mu1, &mu2, &sigma1_sq, &sigma2_sq, &sigma12),
        avg_edgediff: edge_diff_map(w, h, xyb1_ref, &mu1, xyb2_ref, &mu2),
    }
}

/// Compute SSIMULACRA2 score with the B4 parallel horizontal pass.
/// Chunked vertical + rayon-parallel horizontal IIR over rows. Matches
/// cement's `feature = "rayon"` default path (which is what
/// `ssimulacra2 = "0.5"` activates in this workspace).
pub fn ssimulacra2_score_srgb_parallel(
    reference: &[[f32; 3]],
    distorted: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<f64, &'static str> {
    score_inner(reference, distorted, width, height, VerticalKind::ParallelH)
}

/// Compute SSIMULACRA2 score with the B3 reused-scratch path.
/// Chunked vertical pass + a `Scratch` struct holding `mul_buf`,
/// `blur_temp`, and the 5 per-scale Gaussian outputs. Scales reuse
/// the allocation via `truncate` (mirrors cement `Blur::shrink_to`),
/// eliminating the 30-buffer-per-call malloc/zero-fill churn that B2
/// pays.
pub fn ssimulacra2_score_srgb_reuse(
    reference: &[[f32; 3]],
    distorted: &[[f32; 3]],
    width: usize,
    height: usize,
) -> Result<f64, &'static str> {
    if reference.len() != distorted.len() || reference.len() != width * height {
        return Err("dimension mismatch");
    }
    if width < 8 || height < 8 {
        return Err("image too small");
    }
    let mut img1 = build_linear(reference, width, height)?;
    let mut img2 = build_linear(distorted, width, height)?;

    let n0 = width * height;
    let mut scratch = Scratch::new(n0);
    let mut all_scales: Vec<MsssimScale> = Vec::with_capacity(NUM_SCALES);
    let mut cur_w = width;
    let mut cur_h = height;

    for scale in 0..NUM_SCALES {
        if cur_w < 8 || cur_h < 8 {
            break;
        }
        if scale > 0 {
            img1 = downscale_by_2(&img1);
            img2 = downscale_by_2(&img2);
            cur_w = img1.width();
            cur_h = img1.height();
            scratch.shrink_to(cur_w * cur_h);
        }
        all_scales.push(compute_scale_reuse(&img1, &img2, cur_w, cur_h, &mut scratch));
    }
    Ok(aggregate_score(&all_scales))
}

/// Per-image scratch space reused across pyramid scales.
struct Scratch {
    blur_temp: Vec<f32>,
    blur_out_a: Vec<f32>,
    blur_out_b: Vec<f32>,
    blur_out_c: Vec<f32>,
    mul_buf_a: Vec<f32>,
    mul_buf_b: Vec<f32>,
    mul_buf_c: Vec<f32>,
    xyb1_a: Vec<f32>,
    xyb1_b: Vec<f32>,
    xyb1_c: Vec<f32>,
    xyb2_a: Vec<f32>,
    xyb2_b: Vec<f32>,
    xyb2_c: Vec<f32>,
}

impl Scratch {
    fn new(n: usize) -> Self {
        Self {
            blur_temp: vec![0f32; n],
            blur_out_a: vec![0f32; n],
            blur_out_b: vec![0f32; n],
            blur_out_c: vec![0f32; n],
            mul_buf_a: vec![0f32; n],
            mul_buf_b: vec![0f32; n],
            mul_buf_c: vec![0f32; n],
            xyb1_a: vec![0f32; n],
            xyb1_b: vec![0f32; n],
            xyb1_c: vec![0f32; n],
            xyb2_a: vec![0f32; n],
            xyb2_b: vec![0f32; n],
            xyb2_c: vec![0f32; n],
        }
    }
    fn shrink_to(&mut self, n: usize) {
        self.blur_temp.truncate(n);
        self.blur_out_a.truncate(n);
        self.blur_out_b.truncate(n);
        self.blur_out_c.truncate(n);
        self.mul_buf_a.truncate(n);
        self.mul_buf_b.truncate(n);
        self.mul_buf_c.truncate(n);
        self.xyb1_a.truncate(n);
        self.xyb1_b.truncate(n);
        self.xyb1_c.truncate(n);
        self.xyb2_a.truncate(n);
        self.xyb2_b.truncate(n);
        self.xyb2_c.truncate(n);
    }
}

fn compute_scale_reuse(
    img1: &LinearRgb,
    img2: &LinearRgb,
    w: usize,
    h: usize,
    s: &mut Scratch,
) -> MsssimScale {
    // populate xyb planes in-place into scratch.
    fill_positive_xyb_planar(img1, &mut s.xyb1_a, &mut s.xyb1_b, &mut s.xyb1_c);
    fill_positive_xyb_planar(img2, &mut s.xyb2_a, &mut s.xyb2_b, &mut s.xyb2_c);

    let xyb1: [Vec<f32>; 3] = [
        std::mem::take(&mut s.xyb1_a),
        std::mem::take(&mut s.xyb1_b),
        std::mem::take(&mut s.xyb1_c),
    ];
    let xyb2: [Vec<f32>; 3] = [
        std::mem::take(&mut s.xyb2_a),
        std::mem::take(&mut s.xyb2_b),
        std::mem::take(&mut s.xyb2_c),
    ];
    let mut mul_buf: [Vec<f32>; 3] = [
        std::mem::take(&mut s.mul_buf_a),
        std::mem::take(&mut s.mul_buf_b),
        std::mem::take(&mut s.mul_buf_c),
    ];

    let consts = consts::get();

    image_multiply(&xyb1, &xyb1, &mut mul_buf);
    let sigma1_sq = gaussian_blur_reuse(consts, &mul_buf, &mut s.blur_temp, w, h);

    image_multiply(&xyb2, &xyb2, &mut mul_buf);
    let sigma2_sq = gaussian_blur_reuse(consts, &mul_buf, &mut s.blur_temp, w, h);

    image_multiply(&xyb1, &xyb2, &mut mul_buf);
    let sigma12 = gaussian_blur_reuse(consts, &mul_buf, &mut s.blur_temp, w, h);

    let mu1 = gaussian_blur_reuse(consts, &xyb1, &mut s.blur_temp, w, h);
    let mu2 = gaussian_blur_reuse(consts, &xyb2, &mut s.blur_temp, w, h);

    let out = MsssimScale {
        avg_ssim: ssim_map(w, h, &mu1, &mu2, &sigma1_sq, &sigma2_sq, &sigma12),
        avg_edgediff: edge_diff_map(w, h, &xyb1, &mu1, &xyb2, &mu2),
    };

    // return xyb / mul_buf buffers to scratch (so next scale can reuse)
    let [x1a, x1b, x1c] = xyb1;
    s.xyb1_a = x1a; s.xyb1_b = x1b; s.xyb1_c = x1c;
    let [x2a, x2b, x2c] = xyb2;
    s.xyb2_a = x2a; s.xyb2_b = x2b; s.xyb2_c = x2c;
    let [ma, mb, mc] = mul_buf;
    s.mul_buf_a = ma; s.mul_buf_b = mb; s.mul_buf_c = mc;

    out
}

fn fill_positive_xyb_planar(
    lin: &LinearRgb,
    out0: &mut Vec<f32>,
    out1: &mut Vec<f32>,
    out2: &mut Vec<f32>,
) {
    let xyb = Xyb::from(lin.clone());
    let n = xyb.width() * xyb.height();
    out0.clear(); out0.reserve(n);
    out1.clear(); out1.reserve(n);
    out2.clear(); out2.reserve(n);
    for p in xyb.data().iter() {
        out0.push(p[0].mul_add(14.0, 0.42));
        out1.push(p[1] + 0.01);
        out2.push((p[2] - p[1]) + 0.55);
    }
}

fn gaussian_blur_reuse(
    c: &consts::Consts,
    input: &[Vec<f32>; 3],
    tmp: &mut Vec<f32>,
    w: usize,
    h: usize,
) -> [Vec<f32>; 3] {
    let n = w * h;
    let mut out = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    if tmp.len() < n {
        tmp.resize(n, 0f32);
    } else {
        tmp.truncate(n);
    }
    for ch in 0..3 {
        recursive_h(c, &input[ch], tmp, w);
        recursive_v_chunked(c, tmp, &mut out[ch], w, h);
    }
    out
}

#[derive(Copy, Clone)]
enum VerticalKind {
    Single,
    Chunked,
    /// Chunked vertical + rayon-parallel horizontal.
    ParallelH,
}

fn score_inner(
    reference: &[[f32; 3]],
    distorted: &[[f32; 3]],
    width: usize,
    height: usize,
    vk: VerticalKind,
) -> Result<f64, &'static str> {
    if reference.len() != distorted.len() || reference.len() != width * height {
        return Err("dimension mismatch");
    }
    if width < 8 || height < 8 {
        return Err("image too small");
    }
    let mut img1 = build_linear(reference, width, height)?;
    let mut img2 = build_linear(distorted, width, height)?;

    let mut all_scales: Vec<MsssimScale> = Vec::with_capacity(NUM_SCALES);
    let mut cur_w = width;
    let mut cur_h = height;

    for scale in 0..NUM_SCALES {
        if cur_w < 8 || cur_h < 8 {
            break;
        }
        if scale > 0 {
            img1 = downscale_by_2(&img1);
            img2 = downscale_by_2(&img2);
            cur_w = img1.width();
            cur_h = img1.height();
        }
        all_scales.push(compute_scale(&img1, &img2, cur_w, cur_h, vk));
    }

    Ok(aggregate_score(&all_scales))
}

#[derive(Debug, Clone, Copy, Default)]
struct MsssimScale {
    avg_ssim: [f64; 6],
    avg_edgediff: [f64; 12],
}

// --- color conversion helpers ----------------------------------------

fn build_linear(srgb: &[[f32; 3]], w: usize, h: usize) -> Result<LinearRgb, &'static str> {
    let rgb = Rgb::new(
        srgb.to_vec(),
        w,
        h,
        TransferCharacteristic::SRGB,
        ColorPrimaries::BT709,
    )
    .map_err(|_| "rgb conversion")?;
    LinearRgb::try_from(rgb).map_err(|_| "linear conversion")
}

// --- downscale (linear-light box average over 2×2) -------------------

#[inline(always)]
fn downscale_by_2(in_data: &LinearRgb) -> LinearRgb {
    const SCALE: usize = 2;
    let in_w = in_data.width();
    let in_h = in_data.height();
    let out_w = (in_w + SCALE - 1) / SCALE;
    let out_h = (in_h + SCALE - 1) / SCALE;
    let mut out = vec![[0f32; 3]; out_w * out_h];
    let inv = 1f32 / (SCALE * SCALE) as f32;
    let src = in_data.data();
    for oy in 0..out_h {
        for ox in 0..out_w {
            let mut sx = [0f32; 3];
            for iy in 0..SCALE {
                for ix in 0..SCALE {
                    let x = (ox * SCALE + ix).min(in_w - 1);
                    let y = (oy * SCALE + iy).min(in_h - 1);
                    let p = src[y * in_w + x];
                    sx[0] += p[0];
                    sx[1] += p[1];
                    sx[2] += p[2];
                }
            }
            out[oy * out_w + ox] = [sx[0] * inv, sx[1] * inv, sx[2] * inv];
        }
    }
    LinearRgb::new(out, out_w, out_h).expect("dims")
}

// --- per-scale compute ------------------------------------------------

fn compute_scale(
    img1: &LinearRgb,
    img2: &LinearRgb,
    w: usize,
    h: usize,
    vk: VerticalKind,
) -> MsssimScale {
    let xyb1 = make_positive_xyb_planar(img1);
    let xyb2 = make_positive_xyb_planar(img2);

    let mut mul_buf = [
        vec![0f32; w * h],
        vec![0f32; w * h],
        vec![0f32; w * h],
    ];

    image_multiply(&xyb1, &xyb1, &mut mul_buf);
    let sigma1_sq = gaussian_blur(&mul_buf, w, h, vk);

    image_multiply(&xyb2, &xyb2, &mut mul_buf);
    let sigma2_sq = gaussian_blur(&mul_buf, w, h, vk);

    image_multiply(&xyb1, &xyb2, &mut mul_buf);
    let sigma12 = gaussian_blur(&mul_buf, w, h, vk);

    let mu1 = gaussian_blur(&xyb1, w, h, vk);
    let mu2 = gaussian_blur(&xyb2, w, h, vk);

    MsssimScale {
        avg_ssim: ssim_map(w, h, &mu1, &mu2, &sigma1_sq, &sigma2_sq, &sigma12),
        avg_edgediff: edge_diff_map(w, h, &xyb1, &mu1, &xyb2, &mu2),
    }
}

#[inline(always)]
fn make_positive_xyb_planar(lin: &LinearRgb) -> [Vec<f32>; 3] {
    let xyb = Xyb::from(lin.clone());
    let n = xyb.width() * xyb.height();
    let mut planes = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    for (i, p) in xyb.data().iter().enumerate() {
        // Sneyers v2.1 rescale: ensure each component is roughly in 0..1.
        planes[0][i] = p[0].mul_add(14.0, 0.42);
        planes[1][i] = p[1] + 0.01;
        planes[2][i] = (p[2] - p[1]) + 0.55;
    }
    planes
}

#[inline(always)]
fn image_multiply(a: &[Vec<f32>; 3], b: &[Vec<f32>; 3], out: &mut [Vec<f32>; 3]) {
    for c in 0..3 {
        for ((&pa, &pb), o) in a[c].iter().zip(b[c].iter()).zip(out[c].iter_mut()) {
            *o = pa * pb;
        }
    }
}

// --- Gaussian blur (Recursive Gaussian, Charalampidis 2016) -----------
//
// Mirrors the algorithm in ssimulacra2 v0.5.1 `src/blur/gaussian.rs`,
// with constants precomputed at module init for SIGMA = 1.5 (the value
// SSIMULACRA2 uses). Constant derivation lives in `consts::init()` —
// it solves the IIR-pole equations from Charalampidis 2016 §III via a
// 3×3 matrix inversion.

mod consts {
    use std::f64::consts::PI;
    use std::sync::OnceLock;

    pub const SIGMA: f64 = 1.5;

    pub struct Consts {
        pub radius: usize,
        pub mul_in: [f32; 3],
        pub mul_prev: [f32; 3],
        pub mul_prev2: [f32; 3],
        pub vert_mul_in: [f32; 3],
        pub vert_mul_prev: [f32; 3],
    }

    static C: OnceLock<Consts> = OnceLock::new();

    pub fn get() -> &'static Consts {
        C.get_or_init(compute)
    }

    fn compute() -> Consts {
        // Charalampidis 2016 eqs (57), (37), (44), (50), (52), (53),
        // (55), (56), (33). Mirrors ssimulacra2 v0.5.1 build.rs.
        let radius = (3.2795_f64.mul_add(SIGMA, 0.2546)).round();
        let pi_d2r = PI / (2.0 * radius);
        let omega = [pi_d2r, 3.0 * pi_d2r, 5.0 * pi_d2r];

        let p_1 = 1.0 / (0.5 * omega[0]).tan();
        let p_3 = -1.0 / (0.5 * omega[1]).tan();
        let p_5 = 1.0 / (0.5 * omega[2]).tan();

        let r_1 =  p_1 * p_1 / omega[0].sin();
        let r_3 = -p_3 * p_3 / omega[1].sin();
        let r_5 =  p_5 * p_5 / omega[2].sin();

        let neg_half_sigma2 = -0.5 * SIGMA * SIGMA;
        let recip_radius = 1.0 / radius;
        let rho = [
            (neg_half_sigma2 * omega[0] * omega[0]).exp() * recip_radius,
            (neg_half_sigma2 * omega[1] * omega[1]).exp() * recip_radius,
            (neg_half_sigma2 * omega[2] * omega[2]).exp() * recip_radius,
        ];

        let d_13 = p_1.mul_add(r_3, -r_1 * p_3);
        let d_35 = p_3.mul_add(r_5, -r_3 * p_5);
        let d_51 = p_5.mul_add(r_1, -r_5 * p_1);
        let recip_d13 = 1.0 / d_13;
        let zeta_15 = d_35 * recip_d13;
        let zeta_35 = d_51 * recip_d13;

        // Solve A · β = γ; A is the 3×3 matrix from eq (56), γ from
        // eq (55). β are the IIR gains.
        let a = [
            [p_1, p_3, p_5],
            [r_1, r_3, r_5],
            [zeta_15, zeta_35, 1.0],
        ];
        let gamma = [
            1.0,
            radius.mul_add(radius, -SIGMA * SIGMA),
            zeta_15.mul_add(rho[0], zeta_35 * rho[1]) + rho[2],
        ];
        let beta = solve_3x3(&a, &gamma);

        // Sanity check from build.rs.
        let sum = beta[2].mul_add(p_5, beta[0].mul_add(p_1, beta[1] * p_3));
        debug_assert!((sum - 1.0).abs() < 1e-12, "beta normalisation broken: {sum}");

        // Coefficients per eq (33).
        let mut n2 = [0f64; 3];
        let mut d1 = [0f64; 3];
        let mut mul_in_h = [0f64; 3];
        let mut mul_prev_h = [0f64; 3];
        let mut mul_prev2_h = [0f64; 3];
        for i in 0..3 {
            n2[i] = -beta[i] * (omega[i] * (radius + 1.0)).cos();
            d1[i] = -2.0 * omega[i].cos();
            // Horizontal-pass coefficients (first index of build.rs's
            // mul_in[4*i] etc.; the higher indices are for chunked
            // vertical pass at width-4 stride, not needed for
            // single-column horizontal scan).
            mul_in_h[i] = n2[i];
            mul_prev_h[i] = -d1[i];
            mul_prev2_h[i] = -1.0;
        }

        Consts {
            radius: radius as usize,
            mul_in: [mul_in_h[0] as f32, mul_in_h[1] as f32, mul_in_h[2] as f32],
            mul_prev: [mul_prev_h[0] as f32, mul_prev_h[1] as f32, mul_prev_h[2] as f32],
            mul_prev2: [mul_prev2_h[0] as f32, mul_prev2_h[1] as f32, mul_prev2_h[2] as f32],
            vert_mul_in: [n2[0] as f32, n2[1] as f32, n2[2] as f32],
            vert_mul_prev: [d1[0] as f32, d1[1] as f32, d1[2] as f32],
        }
    }

    fn solve_3x3(a: &[[f64; 3]; 3], b: &[f64; 3]) -> [f64; 3] {
        // Cramer's rule; sufficient precision for f64 init time.
        let det =
            a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
          - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
          + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
        let mut x = [0f64; 3];
        for col in 0..3 {
            let mut m = *a;
            for row in 0..3 {
                m[row][col] = b[row];
            }
            let d =
                m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
              - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
              + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            x[col] = d / det;
        }
        x
    }
}

fn gaussian_blur(input: &[Vec<f32>; 3], w: usize, h: usize, vk: VerticalKind) -> [Vec<f32>; 3] {
    let c = consts::get();
    let mut out = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
    let mut tmp = vec![0f32; w * h];
    for ch in 0..3 {
        match vk {
            VerticalKind::Single => {
                recursive_h(c, &input[ch], &mut tmp, w);
                recursive_v(c, &tmp, &mut out[ch], w, h);
            }
            VerticalKind::Chunked => {
                recursive_h(c, &input[ch], &mut tmp, w);
                recursive_v_chunked(c, &tmp, &mut out[ch], w, h);
            }
            VerticalKind::ParallelH => {
                recursive_h_parallel(c, &input[ch], &mut tmp, w);
                recursive_v_chunked(c, &tmp, &mut out[ch], w, h);
            }
        }
    }
    out
}

/// Rayon-parallel horizontal IIR — each row independent, distributed
/// across rayon thread pool. Mirrors cement's `horizontal_pass` with
/// `feature = "rayon"`.
fn recursive_h_parallel(c: &consts::Consts, src: &[f32], dst: &mut [f32], width: usize) {
    use rayon::iter::{IndexedParallelIterator, ParallelIterator};
    use rayon::slice::{ParallelSlice, ParallelSliceMut};
    src.par_chunks_exact(width)
        .zip(dst.par_chunks_exact_mut(width))
        .for_each(|(in_row, out_row)| recursive_h_row(c, in_row, out_row, width));
}

#[inline]
fn recursive_h_row(c: &consts::Consts, src: &[f32], dst: &mut [f32], width: usize) {
    let big_n = c.radius as isize;
    let mut prev_1 = 0f32; let mut prev_3 = 0f32; let mut prev_5 = 0f32;
    let mut prev2_1 = 0f32; let mut prev2_3 = 0f32; let mut prev2_5 = 0f32;

    let mut n = -big_n + 1;
    while n < width as isize {
        let left = n - big_n - 1;
        let right = n + big_n - 1;
        let left_v = if left >= 0 { src[left as usize] } else { 0.0 };
        let right_v = if right < width as isize { src[right as usize] } else { 0.0 };
        let sum = left_v + right_v;

        let mut o1 = sum * c.mul_in[0];
        let mut o3 = sum * c.mul_in[1];
        let mut o5 = sum * c.mul_in[2];

        o1 = c.mul_prev2[0].mul_add(prev2_1, o1);
        o3 = c.mul_prev2[1].mul_add(prev2_3, o3);
        o5 = c.mul_prev2[2].mul_add(prev2_5, o5);
        prev2_1 = prev_1; prev2_3 = prev_3; prev2_5 = prev_5;

        o1 = c.mul_prev[0].mul_add(prev_1, o1);
        o3 = c.mul_prev[1].mul_add(prev_3, o3);
        o5 = c.mul_prev[2].mul_add(prev_5, o5);
        prev_1 = o1; prev_3 = o3; prev_5 = o5;

        if n >= 0 {
            dst[n as usize] = o1 + o3 + o5;
        }
        n += 1;
    }
}

/// Chunked vertical IIR — process `J=128` then `K=32` then 1 column at
/// a time. Mirrors cement `vertical_pass_chunked::<J, K>` for L1
/// locality on tall buffers.
fn recursive_v_chunked(c: &consts::Consts, src: &[f32], dst: &mut [f32], width: usize, height: usize) {
    let mut x = 0usize;
    while x + 128 <= width {
        recursive_v_cols::<128>(c, &src[x..], &mut dst[x..], width, height);
        x += 128;
    }
    while x + 32 <= width {
        recursive_v_cols::<32>(c, &src[x..], &mut dst[x..], width, height);
        x += 32;
    }
    while x < width {
        recursive_v_cols::<1>(c, &src[x..], &mut dst[x..], width, height);
        x += 1;
    }
}

/// Vertical IIR over `COLUMNS` columns at once. State vectors are
/// `3 × COLUMNS × f32`. Reads each row contiguously across the
/// `COLUMNS` columns, then advances down — strided reads coalesce.
///
/// B6 micro-opts on top of B2:
/// - lifts `if n >= 0` (loop-invariant per outer iter) outside the
///   inner column loop so LLVM can auto-vectorise the straight-line
///   compute path
/// - keeps the alternative IIR recurrence cement uses; algebra unchanged
fn recursive_v_cols<const COLUMNS: usize>(
    c: &consts::Consts,
    src: &[f32],
    dst: &mut [f32],
    width: usize,
    height: usize,
) {
    let big_n = c.radius as isize;
    let zeros = [0f32; 128];
    let zeros_view = &zeros[..COLUMNS];

    let mut prev = [0f32; 3 * 128];
    let mut prev2 = [0f32; 3 * 128];
    let mut out_state = [0f32; 3 * 128];

    let pole_span = 3 * COLUMNS;
    let mut n = -big_n + 1;
    while n < height as isize {
        let top = n - big_n - 1;
        let bot = n + big_n - 1;
        let top_row = if top >= 0 {
            &src[top as usize * width..top as usize * width + COLUMNS]
        } else {
            zeros_view
        };
        let bot_row = if bot < height as isize {
            &src[bot as usize * width..bot as usize * width + COLUMNS]
        } else {
            zeros_view
        };

        // compute new_1/3/5 for every column (always — straight-line + auto-vec)
        for i in 0..COLUMNS {
            let i1 = i;
            let i3 = i1 + COLUMNS;
            let i5 = i3 + COLUMNS;
            let sum = top_row[i] + bot_row[i];

            let o1 = prev[i1].mul_add(c.vert_mul_prev[0], prev2[i1]);
            let o3 = prev[i3].mul_add(c.vert_mul_prev[1], prev2[i3]);
            let o5 = prev[i5].mul_add(c.vert_mul_prev[2], prev2[i5]);

            out_state[i1] = sum.mul_add(c.vert_mul_in[0], -o1);
            out_state[i3] = sum.mul_add(c.vert_mul_in[1], -o3);
            out_state[i5] = sum.mul_add(c.vert_mul_in[2], -o5);
        }

        // write to dst only when n is in [0, height) — branch outside inner loop
        if n >= 0 {
            let dst_row = &mut dst[n as usize * width..n as usize * width + COLUMNS];
            for i in 0..COLUMNS {
                dst_row[i] = out_state[i] + out_state[i + COLUMNS] + out_state[i + 2 * COLUMNS];
            }
        }

        // shift prev2 ← prev; prev ← out_state
        prev2[..pole_span].copy_from_slice(&prev[..pole_span]);
        prev[..pole_span].copy_from_slice(&out_state[..pole_span]);

        n += 1;
    }
}

/// Horizontal IIR pass — single-row scan, three poles run in lockstep,
/// final output = sum of the three pole channels.
fn recursive_h(c: &consts::Consts, src: &[f32], dst: &mut [f32], width: usize) {
    let big_n = c.radius as isize;
    let n_rows = src.len() / width;
    for row in 0..n_rows {
        let row_base = row * width;
        let row_src = &src[row_base..row_base + width];
        let row_dst = &mut dst[row_base..row_base + width];

        let mut prev_1 = 0f32; let mut prev_3 = 0f32; let mut prev_5 = 0f32;
        let mut prev2_1 = 0f32; let mut prev2_3 = 0f32; let mut prev2_5 = 0f32;

        let mut n = -big_n + 1;
        while n < width as isize {
            let left = n - big_n - 1;
            let right = n + big_n - 1;
            let left_v = if left >= 0 { row_src[left as usize] } else { 0.0 };
            let right_v = if right < width as isize { row_src[right as usize] } else { 0.0 };
            let sum = left_v + right_v;

            let mut o1 = sum * c.mul_in[0];
            let mut o3 = sum * c.mul_in[1];
            let mut o5 = sum * c.mul_in[2];

            o1 = c.mul_prev2[0].mul_add(prev2_1, o1);
            o3 = c.mul_prev2[1].mul_add(prev2_3, o3);
            o5 = c.mul_prev2[2].mul_add(prev2_5, o5);
            prev2_1 = prev_1; prev2_3 = prev_3; prev2_5 = prev_5;

            o1 = c.mul_prev[0].mul_add(prev_1, o1);
            o3 = c.mul_prev[1].mul_add(prev_3, o3);
            o5 = c.mul_prev[2].mul_add(prev_5, o5);
            prev_1 = o1; prev_3 = o3; prev_5 = o5;

            if n >= 0 {
                row_dst[n as usize] = o1 + o3 + o5;
            }
            n += 1;
        }
    }
}

/// Vertical IIR pass — strides over columns one at a time, using the
/// alternative recurrence from cement's `vertical_pass<1>` (which is
/// algebraically equivalent to the horizontal recurrence but uses
/// different prev/prev2 ordering).
fn recursive_v(c: &consts::Consts, src: &[f32], dst: &mut [f32], width: usize, height: usize) {
    let big_n = c.radius as isize;
    for x in 0..width {
        let mut prev_1 = 0f32; let mut prev_3 = 0f32; let mut prev_5 = 0f32;
        let mut prev2_1 = 0f32; let mut prev2_3 = 0f32; let mut prev2_5 = 0f32;

        let mut n = -big_n + 1;
        while n < height as isize {
            let top = n - big_n - 1;
            let bot = n + big_n - 1;
            let top_v = if top >= 0 { src[top as usize * width + x] } else { 0.0 };
            let bot_v = if bot < height as isize { src[bot as usize * width + x] } else { 0.0 };
            let sum = top_v + bot_v;

            let o1 = prev_1.mul_add(c.vert_mul_prev[0], prev2_1);
            let o3 = prev_3.mul_add(c.vert_mul_prev[1], prev2_3);
            let o5 = prev_5.mul_add(c.vert_mul_prev[2], prev2_5);

            let new_1 = sum.mul_add(c.vert_mul_in[0], -o1);
            let new_3 = sum.mul_add(c.vert_mul_in[1], -o3);
            let new_5 = sum.mul_add(c.vert_mul_in[2], -o5);

            prev2_1 = prev_1; prev2_3 = prev_3; prev2_5 = prev_5;
            prev_1 = new_1; prev_3 = new_3; prev_5 = new_5;

            if n >= 0 {
                dst[n as usize * width + x] = new_1 + new_3 + new_5;
            }
            n += 1;
        }
    }
}

// --- ssim_map (per-scale SSIM error map) -----------------------------

fn ssim_map(
    width: usize,
    height: usize,
    m1: &[Vec<f32>; 3],
    m2: &[Vec<f32>; 3],
    s11: &[Vec<f32>; 3],
    s22: &[Vec<f32>; 3],
    s12: &[Vec<f32>; 3],
) -> [f64; 6] {
    let one_per_pixels = 1.0f64 / (width * height) as f64;
    let mut plane_averages = [0f64; 6];

    for c in 0..3 {
        let mut sum1 = 0.0f64;
        let mut sum4 = 0.0f64;
        for row_idx in 0..height {
            let base = row_idx * width;
            let row_m1 = &m1[c][base..base + width];
            let row_m2 = &m2[c][base..base + width];
            let row_s11 = &s11[c][base..base + width];
            let row_s22 = &s22[c][base..base + width];
            let row_s12 = &s12[c][base..base + width];
            for x in 0..width {
                let mu1 = row_m1[x];
                let mu2 = row_m2[x];
                let mu11 = mu1 * mu1;
                let mu22 = mu2 * mu2;
                let mu12 = mu1 * mu2;
                let mu_diff = mu1 - mu2;
                let num_m = mu_diff.mul_add(-mu_diff, 1.0);
                let num_s = 2f32.mul_add(row_s12[x] - mu12, C2);
                let denom_s = (row_s11[x] - mu11) + (row_s22[x] - mu22) + C2;
                let mut d = 1.0f64 - f64::from((num_m * num_s) / denom_s);
                if d < 0.0 { d = 0.0; }
                sum1 += d;
                sum4 += d.powi(4);
            }
        }
        plane_averages[c * 2] = one_per_pixels * sum1;
        plane_averages[c * 2 + 1] = (one_per_pixels * sum4).sqrt().sqrt();
    }
    plane_averages
}

// --- edge_diff_map (blockiness + smoothness) -------------------------

fn edge_diff_map(
    width: usize,
    height: usize,
    img1: &[Vec<f32>; 3],
    mu1: &[Vec<f32>; 3],
    img2: &[Vec<f32>; 3],
    mu2: &[Vec<f32>; 3],
) -> [f64; 12] {
    let one_per_pixels = 1.0f64 / (width * height) as f64;
    let mut plane_averages = [0f64; 12];

    for c in 0..3 {
        let mut sums = [0f64; 4];
        for row_idx in 0..height {
            let base = row_idx * width;
            let row1 = &img1[c][base..base + width];
            let row2 = &img2[c][base..base + width];
            let rm1 = &mu1[c][base..base + width];
            let rm2 = &mu2[c][base..base + width];
            for x in 0..width {
                let e1 = (row1[x] - rm1[x]).abs() as f64;
                let e2 = (row2[x] - rm2[x]).abs() as f64;
                let d1 = (1.0 + e2) / (1.0 + e1) - 1.0;
                if d1 > 0.0 {
                    sums[0] += d1;
                    sums[1] += d1.powi(4);
                } else {
                    let detail_lost = -d1;
                    sums[2] += detail_lost;
                    sums[3] += detail_lost.powi(4);
                }
            }
        }
        plane_averages[c * 4]     = one_per_pixels * sums[0];
        plane_averages[c * 4 + 1] = (one_per_pixels * sums[1]).sqrt().sqrt();
        plane_averages[c * 4 + 2] = one_per_pixels * sums[2];
        plane_averages[c * 4 + 3] = (one_per_pixels * sums[3]).sqrt().sqrt();
    }
    plane_averages
}

// --- aggregation: 108 weighted sub-scores + polynomial remap ---------

const WEIGHT: [f64; 108] = [
    0.0, 0.000_737_660_670_740_658_6, 0.0, 0.0, 0.000_779_348_168_286_730_9, 0.0, 0.0,
    0.000_437_115_573_010_737_9, 0.0, 1.104_172_642_665_734_6, 0.000_662_848_341_292_71,
    0.000_152_316_327_837_187_52, 0.0, 0.001_640_643_745_659_975_4, 0.0,
    1.842_245_552_053_929_8, 11.441_172_603_757_666, 0.0, 0.000_798_910_943_601_516_3,
    0.000_176_816_438_078_653, 0.0, 1.878_759_497_954_638_7, 10.949_069_906_051_42, 0.0,
    0.000_728_934_699_150_807_2, 0.967_793_708_062_683_3, 0.0,
    0.000_140_034_242_854_358_84, 0.998_176_697_785_496_7,
    0.000_319_497_559_344_350_53, 0.000_455_099_211_379_206_3, 0.0, 0.0,
    0.001_364_876_616_324_339_8, 0.0, 0.0, 0.0, 0.0, 0.0,
    7.466_890_328_078_848, 0.0, 17.445_833_984_131_262,
    0.000_623_560_163_404_146_6, 0.0, 0.0, 6.683_678_146_179_332,
    0.000_377_244_079_796_112_96, 1.027_889_937_768_264, 225.205_153_008_492_74,
    0.0, 0.0, 19.213_238_186_143_016, 0.001_140_152_458_661_836_1,
    0.001_237_755_635_509_985, 176.393_175_984_506_94, 0.0, 0.0,
    24.433_009_998_704_76, 0.285_208_026_121_177_57,
    0.000_448_543_692_383_340_8, 0.0, 0.0, 0.0,
    34.779_063_444_837_72, 44.835_625_328_877_896, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.000_868_055_657_329_169_8, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.000_531_319_187_435_874_7, 0.0,
    0.000_165_338_141_613_791_12, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.000_417_917_180_325_133_6, 0.001_729_082_823_472_283_3, 0.0,
    0.002_082_700_584_663_643_7, 0.0, 0.0,
    8.826_982_764_996_862, 23.192_433_439_989_26, 0.0,
    95.108_049_881_108_6, 0.986_397_803_440_068_2, 0.983_438_279_246_535_3,
    0.001_228_640_504_827_849_3, 171.266_725_589_730_7,
    0.980_785_887_243_537_9, 0.0, 0.0, 0.0,
    0.000_513_006_458_899_067_9, 0.0, 0.000_108_540_578_584_115_37,
];

fn aggregate_score(scales: &[MsssimScale]) -> f64 {
    let mut s = 0.0f64;
    let mut i = 0usize;
    for c in 0..3 {
        for scale in scales {
            for n in 0..2 {
                s = WEIGHT[i].mul_add(scale.avg_ssim[c * 2 + n].abs(), s);
                i += 1;
                s = WEIGHT[i].mul_add(scale.avg_edgediff[c * 4 + n].abs(), s);
                i += 1;
                s = WEIGHT[i].mul_add(scale.avg_edgediff[c * 4 + 2 + n].abs(), s);
                i += 1;
            }
        }
    }
    // Polynomial remap (Sneyers v2.1, identical to ssimulacra2 v0.5.1 §`score`):
    //   step 1: linear scale
    //   step 2: cubic polynomial in `ssim`
    //   step 3: if positive, `ssim.powf(0.628) * -10 + 100`; else 100
    let mut ssim = s * 0.956_238_261_683_484_4_f64;
    ssim = (6.248_496_625_763_138e-5 * ssim * ssim).mul_add(
        ssim,
        2.326_765_642_916_932f64.mul_add(ssim, -0.020_884_521_182_843_837 * ssim * ssim),
    );
    if ssim > 0.0f64 {
        ssim.powf(0.627_633_646_783_138_7).mul_add(-10.0f64, 100.0f64)
    } else {
        100.0f64
    }
}
