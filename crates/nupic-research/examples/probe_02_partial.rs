use std::path::PathBuf;
use anyhow::Result;
use image::ImageReader;
fn workspace_root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf())
}
fn main() -> Result<()> {
    let root = workspace_root()?;
    let p = root.join("assets/png-bench/inputs/02-pluto-transparent.png");
    let img = ImageReader::open(&p)?.with_guessed_format()?.decode()?;
    let r = img.to_rgba8();
    let raw = r.into_raw();
    let n = raw.len() / 4;
    let mut a0 = 0; let mut a255 = 0;
    for px in raw.chunks_exact(4) {
        if px[3] == 0 { a0 += 1; }
        else if px[3] == 255 { a255 += 1; }
    }
    let part = n - a0 - a255;
    println!("02 a0={:.3} a255={:.3} a_partial={:.3}",
        a0 as f64 / n as f64, a255 as f64 / n as f64, part as f64 / n as f64);
    Ok(())
}
