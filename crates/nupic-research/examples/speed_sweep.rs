use std::path::PathBuf;
use std::time::Instant;
use image::ImageReader;
use nupic_quantize::{train_palette_rgba, refine_palette_kmeans, apply_palette_rgba, DEFAULT_REFINE_ITERS};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf();
    for f in ["inputs-ext-real/17-aurora-5mp.png", "inputs-ext-real/25-sofia-cathedral-5mp.png", "inputs-ext-real/27-whale-tail-5mp.png"] {
        let p = root.join("assets/png-bench").join(f);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let w = r.width();
        let h = r.height();
        let raw = r.into_raw();
        let t0 = Instant::now();
        let (pal_ok, pal_a) = train_palette_rgba(&raw, w, h, 256)?;
        let t_train = t0.elapsed().as_secs_f64();
        let t1 = Instant::now();
        let (pal_ok2, pal_a2) = refine_palette_kmeans(&raw, w, h, &pal_ok, &pal_a, DEFAULT_REFINE_ITERS);
        let t_refine = t1.elapsed().as_secs_f64();
        let t2 = Instant::now();
        let _ = apply_palette_rgba(&raw, w, h, &pal_ok2, &pal_a2);
        let t_apply = t2.elapsed().as_secs_f64();
        println!("{:<35} train={:.2}s  refine={:.2}s  apply={:.2}s  total={:.2}s",
            f, t_train, t_refine, t_apply, t_train+t_refine+t_apply);
    }
    Ok(())
}
