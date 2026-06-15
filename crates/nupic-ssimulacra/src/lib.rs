//! `nupic-ssimulacra` — self-built SSIMULACRA2 perceptual quality metric.
//!
//! Stone-layer crate (see `docs/research/png/03b-...md` series).
//! Bit-exact agreement with `ssimulacra2` v0.5.1 (rust-av port) score
//! across the 02-pluto / 04-portrait / 06-landscape fixture set; ~21%
//! faster on M2 via nested rayon (row-level inside horizontal IIR
//! pass, task-level across the σ-chain / μ₁ / μ₂ streams per scale).
//!
//! Algorithm reproduces Sneyers (cloudinary/ssimulacra2 v2.1) +
//! Recursive Gaussian (Charalampidis 2016). 6-scale linear-light
//! pyramid, XYB color space, SSIM + asymmetric edge-diff maps,
//! 108-term weighted aggregation, polynomial remap to 0..=100.
//!
//! Public API entry points:
//!
//! - [`ssimulacra2_score`] — 8-bit RGBA sRGB in, `f64` score out
//! - [`ssimulacra2_score_f32`] — `[f32; 3]` per-pixel sRGB in
//!
//! ```no_run
//! # use nupic_ssimulacra::ssimulacra2_score;
//! let reference: Vec<u8> = /* RGBA bytes from a decoded PNG */ vec![];
//! let distorted: Vec<u8> = /* same dimensions */ vec![];
//! let score = ssimulacra2_score(&reference, &distorted, 800, 600).unwrap();
//! assert!(score >= 0.0 && score <= 100.0);
//! ```

#![allow(clippy::excessive_precision)]
#![allow(clippy::inline_always)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]

use yuvxyb::{ColorPrimaries, LinearRgb, Rgb, TransferCharacteristic, Xyb};

const NUM_SCALES: usize = 6;
const C2: f32 = 0.0009;

/// SSIMULACRA2 score for two same-size sRGB RGBA8 buffers.
///
/// `reference` / `distorted` are tightly packed `[r, g, b, a, ...]`
/// of length `4 * width * height`. Alpha is dropped — SSIMULACRA2
/// operates on linear-light XYB, callers handle alpha separately.
///
/// Returns score in `(-∞, 100]`; 100 = identical; 90 = visually
/// indistinguishable; below 0 = catastrophic (see Sneyers 2023
/// calibration table).
///
/// # Errors
///
/// - `"dimension mismatch"`:`reference` / `distorted` lengths不一致 或
///   不等于 `4 * width * height`
/// - `"image too small"`:width 或 height < 8(SSIMULACRA2 spec 限制)
pub fn ssimulacra2_score(
    reference: &[u8],
    distorted: &[u8],
    width: u32,
    height: u32,
) -> Result<f64, &'static str> {
    let expected = (width as usize) * (height as usize) * 4;
    if reference.len() != expected || distorted.len() != expected {
        return Err("dimension mismatch");
    }
    let r_f32: Vec<[f32; 3]> = reference
        .chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    let d_f32: Vec<[f32; 3]> = distorted
        .chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    ssimulacra2_score_f32(&r_f32, &d_f32, width as usize, height as usize)
}

/// SSIMULACRA2 score taking f32 sRGB triples (`[r, g, b]` per pixel).
/// Use when you've already done sRGB normalisation or you have a 24-bit
/// RGB buffer rather than RGBA.
pub fn ssimulacra2_score_f32(
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
        all_scales.push(compute_scale(&img1, &img2, cur_w, cur_h));
    }
    Ok(aggregate_score(&all_scales))
}

// --- internal types ---------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct MsssimScale {
    avg_ssim: [f64; 6],
    avg_edgediff: [f64; 12],
}

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

fn compute_scale(img1: &LinearRgb, img2: &LinearRgb, w: usize, h: usize) -> MsssimScale {
    let xyb1 = make_positive_xyb_planar(img1);
    let xyb2 = make_positive_xyb_planar(img2);

    let xyb1_ref = &xyb1;
    let xyb2_ref = &xyb2;

    // 3 parallel streams: σ-chain + μ₁ + μ₂. Inner gaussian_blur uses
    // rayon-parallel horizontal IIR pass.
    let ((sigma1_sq, sigma2_sq, sigma12), (mu1, mu2)) = rayon::join(
        || {
            let mut mul_buf = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
            image_multiply(xyb1_ref, xyb1_ref, &mut mul_buf);
            let s1 = gaussian_blur(&mul_buf, w, h);
            image_multiply(xyb2_ref, xyb2_ref, &mut mul_buf);
            let s2 = gaussian_blur(&mul_buf, w, h);
            image_multiply(xyb1_ref, xyb2_ref, &mut mul_buf);
            let s12 = gaussian_blur(&mul_buf, w, h);
            (s1, s2, s12)
        },
        || rayon::join(
            || gaussian_blur(xyb1_ref, w, h),
            || gaussian_blur(xyb2_ref, w, h),
        ),
    );

    MsssimScale {
        avg_ssim: ssim_map(w, h, &mu1, &mu2, &sigma1_sq, &sigma2_sq, &sigma12),
        avg_edgediff: edge_diff_map(w, h, xyb1_ref, &mu1, xyb2_ref, &mu2),
    }
}

#[inline(always)]
fn make_positive_xyb_planar(lin: &LinearRgb) -> [Vec<f32>; 3] {
    let xyb = Xyb::from(lin.clone());
    let n = xyb.width() * xyb.height();
    let mut planes = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    for (i, p) in xyb.data().iter().enumerate() {
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

// --- Recursive Gaussian (Charalampidis 2016) ---

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
    pub fn get() -> &'static Consts { C.get_or_init(compute) }

    fn compute() -> Consts {
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
        let a = [[p_1, p_3, p_5], [r_1, r_3, r_5], [zeta_15, zeta_35, 1.0]];
        let gamma = [
            1.0,
            radius.mul_add(radius, -SIGMA * SIGMA),
            zeta_15.mul_add(rho[0], zeta_35 * rho[1]) + rho[2],
        ];
        let beta = solve_3x3(&a, &gamma);
        let mut n2 = [0f64; 3];
        let mut d1 = [0f64; 3];
        for i in 0..3 {
            n2[i] = -beta[i] * (omega[i] * (radius + 1.0)).cos();
            d1[i] = -2.0 * omega[i].cos();
        }
        Consts {
            radius: radius as usize,
            mul_in: [n2[0] as f32, n2[1] as f32, n2[2] as f32],
            mul_prev: [(-d1[0]) as f32, (-d1[1]) as f32, (-d1[2]) as f32],
            mul_prev2: [-1.0, -1.0, -1.0],
            vert_mul_in: [n2[0] as f32, n2[1] as f32, n2[2] as f32],
            vert_mul_prev: [d1[0] as f32, d1[1] as f32, d1[2] as f32],
        }
    }

    fn solve_3x3(a: &[[f64; 3]; 3], b: &[f64; 3]) -> [f64; 3] {
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

fn gaussian_blur(input: &[Vec<f32>; 3], w: usize, h: usize) -> [Vec<f32>; 3] {
    let c = consts::get();
    let mut out = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
    let mut tmp = vec![0f32; w * h];
    for ch in 0..3 {
        recursive_h_parallel(c, &input[ch], &mut tmp, w);
        recursive_v_chunked(c, &tmp, &mut out[ch], w, h);
    }
    out
}

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
        if n >= 0 { dst[n as usize] = o1 + o3 + o5; }
        n += 1;
    }
}

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

fn recursive_v_cols<const COLUMNS: usize>(
    c: &consts::Consts, src: &[f32], dst: &mut [f32], width: usize, height: usize,
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
        let top_row = if top >= 0 { &src[top as usize * width..top as usize * width + COLUMNS] } else { zeros_view };
        let bot_row = if bot < height as isize { &src[bot as usize * width..bot as usize * width + COLUMNS] } else { zeros_view };
        for i in 0..COLUMNS {
            let i1 = i; let i3 = i1 + COLUMNS; let i5 = i3 + COLUMNS;
            let sum = top_row[i] + bot_row[i];
            let o1 = prev[i1].mul_add(c.vert_mul_prev[0], prev2[i1]);
            let o3 = prev[i3].mul_add(c.vert_mul_prev[1], prev2[i3]);
            let o5 = prev[i5].mul_add(c.vert_mul_prev[2], prev2[i5]);
            out_state[i1] = sum.mul_add(c.vert_mul_in[0], -o1);
            out_state[i3] = sum.mul_add(c.vert_mul_in[1], -o3);
            out_state[i5] = sum.mul_add(c.vert_mul_in[2], -o5);
        }
        if n >= 0 {
            let dst_row = &mut dst[n as usize * width..n as usize * width + COLUMNS];
            for i in 0..COLUMNS {
                dst_row[i] = out_state[i] + out_state[i + COLUMNS] + out_state[i + 2 * COLUMNS];
            }
        }
        prev2[..pole_span].copy_from_slice(&prev[..pole_span]);
        prev[..pole_span].copy_from_slice(&out_state[..pole_span]);
        n += 1;
    }
}

// --- ssim + edge_diff maps + aggregation ---

fn ssim_map(
    width: usize, height: usize,
    m1: &[Vec<f32>; 3], m2: &[Vec<f32>; 3],
    s11: &[Vec<f32>; 3], s22: &[Vec<f32>; 3], s12: &[Vec<f32>; 3],
) -> [f64; 6] {
    let one_per_pixels = 1.0f64 / (width * height) as f64;
    let mut plane_averages = [0f64; 6];
    for c in 0..3 {
        let mut sum1 = 0.0f64; let mut sum4 = 0.0f64;
        for row_idx in 0..height {
            let base = row_idx * width;
            for x in 0..width {
                let i = base + x;
                let mu1 = m1[c][i]; let mu2 = m2[c][i];
                let mu11 = mu1 * mu1; let mu22 = mu2 * mu2; let mu12 = mu1 * mu2;
                let mu_diff = mu1 - mu2;
                let num_m = mu_diff.mul_add(-mu_diff, 1.0);
                let num_s = 2f32.mul_add(s12[c][i] - mu12, C2);
                let denom_s = (s11[c][i] - mu11) + (s22[c][i] - mu22) + C2;
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

fn edge_diff_map(
    width: usize, height: usize,
    img1: &[Vec<f32>; 3], mu1: &[Vec<f32>; 3],
    img2: &[Vec<f32>; 3], mu2: &[Vec<f32>; 3],
) -> [f64; 12] {
    let one_per_pixels = 1.0f64 / (width * height) as f64;
    let mut plane_averages = [0f64; 12];
    for c in 0..3 {
        let mut sums = [0f64; 4];
        for row_idx in 0..height {
            let base = row_idx * width;
            for x in 0..width {
                let i = base + x;
                let e1 = (img1[c][i] - mu1[c][i]).abs() as f64;
                let e2 = (img2[c][i] - mu2[c][i]).abs() as f64;
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
    let mut ssim = s * 0.956_238_261_683_484_4_f64;
    ssim = (6.248_496_625_763_138e-5 * ssim * ssim).mul_add(
        ssim,
        2.326_765_642_916_932f64.mul_add(ssim, -0.020_884_521_182_843_837 * ssim * ssim),
    );
    if ssim > 0.0 {
        ssim.powf(0.627_633_646_783_138_7).mul_add(-10.0, 100.0)
    } else {
        100.0
    }
}
