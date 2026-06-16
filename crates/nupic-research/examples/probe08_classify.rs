//! Probe 08-gradient-large classify result + branch.

use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    for fname in [
        "inputs-ext/08-gradient-large.png",
        "inputs-ext/11-photo-noisy.png",
        "inputs-ext/13-very-large-photo.png",
        "inputs-ext/14-soft-transparent.png",
    ] {
        let path = root.join("assets/png-bench").join(fname);
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let raw = rgba.into_raw();
        let mut n_opaque = 0usize;
        let mut n_total = 0usize;
        for px in raw.chunks_exact(4) {
            n_total += 1;
            if px[3] == 255 { n_opaque += 1; }
        }
        let opq = n_opaque as f64 / n_total as f64;
        let d = nupic_quantize::classify_for_auto_dither(&raw, w);
        println!("{:<45} {}x{}  opq={:.3}  classify_d={:.3}",
            fname, w, h, opq, d);
    }
    Ok(())
}
