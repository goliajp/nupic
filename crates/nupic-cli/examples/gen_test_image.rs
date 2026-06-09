//! Generate a deterministic test PNG for end-to-end CLI verification.
//! Usage: cargo run --example gen_test_image -- <out.png>

use std::env;

fn main() -> anyhow::Result<()> {
    let out = env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/nupic-test/source.png".to_string());

    let (w, h) = (800u32, 600u32);
    let mut img = image::RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let r = ((x * 255) / w) as u8;
        let g = ((y * 255) / h) as u8;
        let b = (((x + y) * 255) / (w + h)) as u8;
        *p = image::Rgba([r, g, b, 255]);
    }
    // Filled rectangle (solid color, helps compression test).
    for y in 100..300 {
        for x in 100..300 {
            img.put_pixel(x, y, image::Rgba([220, 38, 38, 255]));
        }
    }
    // Filled disk.
    let (cx, cy, r) = (550.0f32, 350.0f32, 150.0f32);
    for y in 200..500 {
        for x in 400..700 {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= r * r {
                img.put_pixel(x, y, image::Rgba([37, 99, 235, 255]));
            }
        }
    }
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent)?;
    }
    img.save(&out)?;
    println!("wrote {out}");
    Ok(())
}
