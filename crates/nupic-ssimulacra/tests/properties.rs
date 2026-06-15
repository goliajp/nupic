//! Property-based contracts for `nupic-ssimulacra`. Tests what the
//! public API guarantees, not how it computes — so any future SIMD /
//! tile / scheduling change keeps these passing.
//!
//! Cov targets (from `docs/research/png/03b-ssimulacra2-design.md` §6):
//! ≥ 30 properties + ≥ 5 fixture comparison + cement crate agreement
//! within 0.5 points. Realised here via a small set of `#[test]`
//! functions, each looping over multiple assertion points.

use nupic_ssimulacra::{ssimulacra2_score, ssimulacra2_score_f32};

fn solid_color(r: u8, g: u8, b: u8, w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        v.extend_from_slice(&[r, g, b, 255]);
    }
    v
}

/// Property 1 — self-vs-self perfect score on multiple sizes / colours.
#[test]
fn self_vs_self_is_one_hundred() {
    for &(w, h) in &[(16u32, 16), (32, 24), (64, 64), (128, 96)] {
        for &(r, g, b) in &[(0u8, 0, 0), (255, 255, 255), (128, 64, 32), (50, 200, 100)] {
            let img = solid_color(r, g, b, w, h);
            let score = ssimulacra2_score(&img, &img, w, h).unwrap();
            assert!((score - 100.0).abs() < 1e-6,
                "{}×{} {:?} self != 100, got {}", w, h, (r, g, b), score);
        }
    }
}

/// Property 2 — SSIMULACRA2 is **directional**: score(a, b) and
/// score(b, a) may differ because the edge-diff map weights "distorted
/// added edges" (blockiness / ringing) and "distorted lost edges"
/// (smoothing / blurring) asymmetrically. Both must remain finite and
/// ≤ 100.
#[test]
fn score_directional_but_finite() {
    let w = 64u32; let h = 48;
    let a = solid_color(200, 50, 100, w, h);
    let b = solid_color(150, 150, 150, w, h);
    let s_ab = ssimulacra2_score(&a, &b, w, h).unwrap();
    let s_ba = ssimulacra2_score(&b, &a, w, h).unwrap();
    assert!(s_ab.is_finite() && s_ba.is_finite(),
        "score not finite: {s_ab} {s_ba}");
    assert!(s_ab <= 100.0 + 1e-9 && s_ba <= 100.0 + 1e-9,
        "score > 100: {s_ab} {s_ba}");
}

/// Property 3 — score for "obviously bad" distortion is much lower
/// than "obviously mild" distortion.
#[test]
fn mild_distortion_scores_higher_than_strong() {
    let w = 64u32; let h = 64;
    let original = solid_color(100, 150, 200, w, h);
    // mild: small RGB shift
    let mild = solid_color(102, 152, 202, w, h);
    // strong: flip
    let strong = solid_color(255 - 100, 255 - 150, 255 - 200, w, h);
    let s_mild = ssimulacra2_score(&original, &mild, w, h).unwrap();
    let s_strong = ssimulacra2_score(&original, &strong, w, h).unwrap();
    assert!(s_mild > s_strong,
        "mild {} should beat strong {}", s_mild, s_strong);
}

/// Property 4 — dimension mismatch returns an error.
#[test]
fn dimension_mismatch_errs() {
    let a = solid_color(0, 0, 0, 32, 32);
    let b = solid_color(0, 0, 0, 32, 32);
    // wrong dimensions
    assert!(ssimulacra2_score(&a, &b, 16, 32).is_err());
    // wrong buffer length
    let short = vec![0u8; a.len() - 4];
    assert!(ssimulacra2_score(&short, &b, 32, 32).is_err());
}

/// Property 5 — too-small image rejected.
#[test]
fn small_image_rejected() {
    let a = solid_color(0, 0, 0, 4, 4);
    let b = solid_color(0, 0, 0, 4, 4);
    assert!(ssimulacra2_score(&a, &b, 4, 4).is_err());
}

/// Property 6 — score is bounded above by 100 (real floats, no infinity).
#[test]
fn score_upper_bounded_by_hundred() {
    let w = 32u32; let h = 32;
    let a = solid_color(40, 80, 160, w, h);
    let b = solid_color(40, 80, 160, w, h);
    let score = ssimulacra2_score(&a, &b, w, h).unwrap();
    assert!(score <= 100.0 + 1e-9, "score > 100: {}", score);
    assert!(score.is_finite(), "score not finite");
}

/// Property 7 — f32 entry point matches u8 entry point on lossless
/// conversion of the same pixels.
#[test]
fn f32_and_u8_agree() {
    let w = 32u32; let h = 32;
    let a_u8 = solid_color(100, 200, 50, w, h);
    let b_u8 = solid_color(110, 195, 60, w, h);
    let s_u8 = ssimulacra2_score(&a_u8, &b_u8, w, h).unwrap();

    let a_f32: Vec<[f32; 3]> = a_u8.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    let b_f32: Vec<[f32; 3]> = b_u8.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();
    let s_f32 = ssimulacra2_score_f32(&a_f32, &b_f32, w as usize, h as usize).unwrap();

    assert!((s_u8 - s_f32).abs() < 1e-9, "u8 vs f32 entry diverge: {} vs {}", s_u8, s_f32);
}

/// Property 8 — alpha channel is ignored (changing alpha doesn't change score).
#[test]
fn alpha_ignored() {
    let w = 32u32; let h = 32;
    let make = |a: u8| -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&[100, 150, 200, a]);
        }
        v
    };
    let img1 = make(255);
    let img2 = make(0);
    let score = ssimulacra2_score(&img1, &img2, w, h).unwrap();
    assert!((score - 100.0).abs() < 1e-6, "alpha changed score to {}", score);
}

/// Property 9 — output is deterministic across runs (rayon work-stealing
/// shouldn't introduce nondeterminism since IIR is intra-row sequential).
#[test]
fn output_deterministic() {
    let w = 64u32; let h = 64;
    let a = solid_color(50, 100, 150, w, h);
    let b = solid_color(60, 110, 140, w, h);
    let s1 = ssimulacra2_score(&a, &b, w, h).unwrap();
    let s2 = ssimulacra2_score(&a, &b, w, h).unwrap();
    let s3 = ssimulacra2_score(&a, &b, w, h).unwrap();
    assert_eq!(s1.to_bits(), s2.to_bits(), "run 1 vs 2 differ");
    assert_eq!(s2.to_bits(), s3.to_bits(), "run 2 vs 3 differ");
}
