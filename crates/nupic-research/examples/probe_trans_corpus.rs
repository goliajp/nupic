//! Probe signals on partial-transparent fixtures 14/21/22/23.

use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;
use std::collections::HashSet;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    for (subdir, name) in [
        ("inputs", "01-png-transparency-demo.png"),
        ("inputs-ext", "12-tiny-icon.png"),
        ("inputs-ext", "14-soft-transparent.png"),
        ("inputs-ext-real", "21-earth-hemisphere-trans.png"),
        ("inputs-ext-real", "22-tree-trans.png"),
        ("inputs-ext-real", "23-statue-liberty-trans.png"),
    ] {
        let p = root.join("assets/png-bench").join(subdir).join(name);
        let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
        let r = img.to_rgba8();
        let (w, h) = (r.width(), r.height());
        let raw = r.into_raw();
        let n_total = raw.len() / 4;
        let mut n_opq = 0usize;
        let mut alphas = vec![0u32; 256];
        for p in raw.chunks_exact(4) {
            if p[3] == 255 { n_opq += 1; }
            alphas[p[3] as usize] += 1;
        }
        let opq = n_opq as f64 / n_total as f64;
        let alpha_zero = alphas[0] as f64 / n_total as f64;
        let alpha_partial = (n_total - n_opq - alphas[0] as usize) as f64 / n_total as f64;
        let mut uniq: HashSet<u32> = HashSet::new();
        for p in raw.chunks_exact(4) {
            if p[3] == 255 {
                uniq.insert((p[0] as u32) | ((p[1] as u32)<<8) | ((p[2] as u32)<<16));
                if uniq.len() >= 1_000_000 { break; }
            }
        }
        let d = nupic_quantize::classify_for_auto_dither(&raw, w);
        println!("{:<32} {}x{} opq={:.3} a0={:.3} apart={:.3} uniq={:>9} classify_d={:.2}",
            name, w, h, opq, alpha_zero, alpha_partial, uniq.len(), d);
    }
    Ok(())
}
