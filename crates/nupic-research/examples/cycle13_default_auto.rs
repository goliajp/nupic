//! Cycle 13 — should --dither default flip from `off` to `auto`?
//! With Cycle 11's tier-4 split, auto should be net-positive on photos
//! and neutral on UI/logo/transparent. Validate on full 7-fixture corpus.

use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::Result;
use image::ImageReader;
use nupic_quantize::{QuantizeOpts, quantize_indexed_png};

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn ssimulacra2(orig: &Path, cmp: &Path) -> f64 {
    let out = Command::new("nupic")
        .args(["compare", "-m", "ssimulacra2"])
        .arg(orig).arg(cmp).output().expect("nupic compare");
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find_map(|l| {
        l.strip_prefix("SSIMULACRA2: ")
            .and_then(|v| v.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())
    }).unwrap_or(0.0)
}

fn enc(raw: &[u8], w: u32, h: u32, strength: f32, src: &Path, tmpdir: &Path, label: &str)
    -> Result<(usize, f64)>
{
    let opts = QuantizeOpts {
        n_colors: 256, oxipng_preset: 5, strip_metadata: true,
        dither_strength: strength,
            ..Default::default()
        };
    let png = quantize_indexed_png(raw, w, h, opts).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let out = tmpdir.join(format!("{label}.png"));
    std::fs::write(&out, &png)?;
    Ok((png.len(), ssimulacra2(src, &out)))
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "01-png-transparency-demo.png",
        "02-pluto-transparent.png",
        "03-wikipedia-logo.png",
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];
    let tmpdir = std::env::temp_dir().join("cycle13");
    std::fs::create_dir_all(&tmpdir)?;
    println!("--dither off vs auto on 7-fixture corpus");
    println!("{:<32} {:>10} {:>10}    {:>10} {:>10}    {:>10} {:>10}",
        "fixture", "off_size", "off_SSIM", "auto_size", "auto_SSIM", "Δ size", "Δ SSIM");
    let mut sum_off=0.0; let mut sum_auto=0.0;
    let mut sum_sz_off=0i64; let mut sum_sz_auto=0i64;
    for f in &fixtures {
        let p = root.join("assets/png-bench/inputs").join(f);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let (s_off, q_off) = enc(&raw, w, h, 0.0, &p, &tmpdir, &format!("{f}-off"))?;
        let (s_au, q_au) = enc(&raw, w, h, f32::NAN, &p, &tmpdir, &format!("{f}-auto"))?;
        let ds = s_au as i64 - s_off as i64;
        let dq = q_au - q_off;
        sum_off += q_off; sum_auto += q_au;
        sum_sz_off += s_off as i64; sum_sz_auto += s_au as i64;
        println!("{:<32} {:>10} {:>10.3}    {:>10} {:>10.3}    {:>+10} {:>+10.3}",
            f, s_off, q_off, s_au, q_au, ds, dq);
    }
    let n = 7.0;
    println!();
    println!("Mean SSIM: off={:.3}, auto={:.3}, Δ={:+.3}",
        sum_off/n, sum_auto/n, (sum_auto-sum_off)/n);
    println!("Total bytes: off={}, auto={}, Δ={:+} ({:+.2}%)",
        sum_sz_off, sum_sz_auto, sum_sz_auto - sum_sz_off,
        (sum_sz_auto - sum_sz_off) as f64 / sum_sz_off as f64 * 100.0);
    Ok(())
}
