//! Cycle 37 part 3: subsample stride sweep.
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use image::ImageReader;
use nupic_quantize::{
    train_palette_rgba, refine_palette_kmeans_instrumented_strided,
    apply_palette_rgba, encode_indexed_png_with_alpha,
};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    let nupic = root.join("target/release/nupic");
    let tmp = std::env::temp_dir();
    let fixtures = [
        ("inputs/04-photo-portrait.png", "04"),
        ("inputs/05-photo-mountain.png", "05"),
        ("inputs/06-photo-landscape.png", "06"),
        ("inputs/07-photo-product.png", "07"),
        ("inputs-ext-real/17-aurora-5mp.png", "17"),
        ("inputs-ext-real/25-sofia-cathedral-5mp.png", "25"),
        ("inputs-ext-real/27-whale-tail-5mp.png", "27"),
    ];
    let strides: [usize; 5] = [1, 2, 4, 8, 16];
    println!("{:<6} {:>6} {:>8} {:>8} {:>10}", "fix", "stride", "iters", "time_s", "ssim");
    for (rel, lbl) in fixtures {
        let p = root.join("assets/png-bench").join(rel);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width(); let h = r.height();
        let raw = r.into_raw();
        for &stride in &strides {
            let (pal_init, alpha_init) = train_palette_rgba(&raw, w, h, 256)?;
            let t0 = Instant::now();
            let (pal, alpha, iters) = refine_palette_kmeans_instrumented_strided(
                &raw, w, h, &pal_init, &alpha_init, 100, 0.0005, stride);
            let dt = t0.elapsed().as_secs_f64();
            let (indices, palette_srgb) = apply_palette_rgba(&raw, w, h, &pal, &alpha);
            let trns = if alpha.iter().all(|&a| a == 255) { None } else { Some(alpha.as_slice()) };
            let raw_png = encode_indexed_png_with_alpha(w, h, &indices, &palette_srgb, trns)?;
            let out = tmp.join(format!("st_{}_s{}.png", lbl, stride));
            std::fs::write(&out, &raw_png)?;
            let cmp_out = Command::new(&nupic)
                .args(["compare", "-m", "ssimulacra2"])
                .arg(&p).arg(&out).output()?;
            let s = String::from_utf8_lossy(&cmp_out.stdout);
            let ssim: f64 = s.lines()
                .find_map(|l| l.strip_prefix("SSIMULACRA2: ").and_then(|v| v.split_whitespace().next()).and_then(|n| n.parse().ok()))
                .unwrap_or(0.0);
            println!("{:<6} {:>6} {:>8} {:>8.2} {:>10.3}", lbl, stride, iters, dt, ssim);
        }
    }
    Ok(())
}
