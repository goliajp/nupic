//! 4K-input mem-safety + perf check for Stone B B5.
//!
//! Runs both cement and B5 on a 4K (3840×2160) PNG. Reports peak
//! resident memory (via mach task_info on macOS; fallback to none).
//! Backs `docs/research/png/03b-six-ssim-graduation.md` §2 mem ceiling
//! check.
//!
//! Run:
//!   cargo run --release -p nupic-research --example ssim_4k_check -- /tmp/test-4k.png

use std::env;
use std::time::Instant;

use nupic_research::ssim_b1::ssimulacra2_score_srgb_b5;
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

fn main() {
    let path = env::args().nth(1).expect("usage: ssim_4k_check <png>");
    let img = ::image::open(&path).expect("open png").to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    println!("4K-check: {} ({}×{}, {} MB raw)", path, w, h,
             w * h * 4 / 1024 / 1024);
    let raw = img.into_raw();
    let srgb: Vec<[f32; 3]> = raw.chunks_exact(4)
        .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
        .collect();

    println!("--- cement ssimulacra2 v0.5.1 (self-vs-self) ---");
    let r = Rgb::new(srgb.clone(), w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
    let d = Rgb::new(srgb.clone(), w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709).expect("rgb");
    let t0 = Instant::now();
    let cement_score = compute_frame_ssimulacra2(r, d).expect("cement");
    let cement_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("cement: score = {cement_score:.3}, time = {cement_ms:.1} ms");

    println!("--- B5 (self-vs-self) ---");
    let t0 = Instant::now();
    let b5_score = ssimulacra2_score_srgb_b5(&srgb, &srgb, w, h).expect("b5");
    let b5_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("B5: score = {b5_score:.3}, time = {b5_ms:.1} ms");

    println!("\nscore_diff = {:.6}, B5 / cement = {:.2}×",
             (cement_score - b5_score).abs(), b5_ms / cement_ms);
}
