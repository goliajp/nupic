//! Generate extended corpus fixtures to expose new ceilings.
//! Covers: large-photo, gradient, noisy, comic-flat, icon, mixed.
//! Saved to assets/png-bench/inputs-ext/.

use std::path::PathBuf;
use anyhow::Result;
use image::{ImageBuffer, Rgba, RgbaImage, ImageReader, imageops};

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

// 08-gradient-large: 2400×1600 (3.84 MP) horizontal+vertical color gradient
// — large + tier-4 (smooth-only) — tests > 4MP sampling path + banding susceptibility
fn gen_gradient_large(w: u32, h: u32) -> RgbaImage {
    ImageBuffer::from_fn(w, h, |x, y| {
        let r = (x * 255 / w.max(1)) as u8;
        let g = (y * 255 / h.max(1)) as u8;
        let b = ((x + y) * 255 / (w + h).max(1)) as u8;
        Rgba([r, g, b, 255])
    })
}

// 09-ui-checker-text: 1920×1080 with checker background + colored solids
// + simulated text bars — tier-3 (mean_run > 2)
fn gen_ui_checker(w: u32, h: u32) -> RgbaImage {
    let mut img = ImageBuffer::from_fn(w, h, |x, y| {
        let cell = ((x / 16) ^ (y / 16)) & 1;
        if cell == 0 { Rgba([240, 240, 240, 255]) } else { Rgba([200, 200, 200, 255]) }
    });
    // Solid color block top-left
    for y in 50..200 {
        for x in 50..400 {
            *img.get_pixel_mut(x, y) = Rgba([66, 133, 244, 255]); // Google blue
        }
    }
    // Red block
    for y in 250..350 {
        for x in 50..600 {
            *img.get_pixel_mut(x, y) = Rgba([234, 67, 53, 255]);
        }
    }
    // Text-like stripes
    for y in 500..520 {
        for x in 50..1800 {
            if (x / 4) & 1 == 0 {
                *img.get_pixel_mut(x, y) = Rgba([33, 33, 33, 255]);
            }
        }
    }
    img
}

// 10-comic-flat: 1200×900 large flat color regions + sharp edges
// — comic / illustration class (limited colors + hard edges)
fn gen_comic_flat(w: u32, h: u32) -> RgbaImage {
    ImageBuffer::from_fn(w, h, |x, y| {
        // 4-quadrant flat color, with a circle overlay
        let cx = (w / 2) as i32;
        let cy = (h / 2) as i32;
        let dx = x as i32 - cx;
        let dy = y as i32 - cy;
        let dist_sq = dx*dx + dy*dy;
        let r = (cx.min(cy) * 7 / 10).pow(2);
        let in_circle = dist_sq < r;
        let q = if x < w/2 { if y < h/2 { 0 } else { 1 } } else { if y < h/2 { 2 } else { 3 } };
        let bg = match q {
            0 => Rgba([255, 200, 100, 255]),
            1 => Rgba([100, 200, 255, 255]),
            2 => Rgba([200, 100, 255, 255]),
            _ => Rgba([100, 255, 200, 255]),
        };
        if in_circle { Rgba([40, 40, 40, 255]) } else { bg }
    })
}

// 11-photo-noisy: derived from 05-mountain + per-pixel noise.
// Tests tier-4 photo classifier on noise-heavy content (var should
// be even higher than 05's natural 320).
fn gen_photo_noisy(src: &RgbaImage) -> RgbaImage {
    let mut img = src.clone();
    let mut rng = 0xC0DEu64;
    for p in img.pixels_mut() {
        for c in 0..3 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let noise = ((rng >> 50) & 0x1F) as i32 - 16;
            let v = (p.0[c] as i32 + noise).clamp(0, 255) as u8;
            p.0[c] = v;
        }
    }
    img
}

// 12-tiny-icon: 64×64 fully opaque colorful icon (tier-1 small)
fn gen_tiny_icon() -> RgbaImage {
    ImageBuffer::from_fn(64, 64, |x, y| {
        let cx = 32i32 - x as i32;
        let cy = 32i32 - y as i32;
        let d = ((cx*cx + cy*cy) as f64).sqrt();
        if d < 25.0 {
            let theta = (cy as f64).atan2(cx as f64);
            let h = ((theta + std::f64::consts::PI) * 6.0 / (2.0 * std::f64::consts::PI)) as u8;
            match h % 6 {
                0 => Rgba([255, 80, 80, 255]),
                1 => Rgba([255, 200, 80, 255]),
                2 => Rgba([80, 255, 80, 255]),
                3 => Rgba([80, 200, 255, 255]),
                4 => Rgba([80, 80, 255, 255]),
                _ => Rgba([200, 80, 255, 255]),
            }
        } else {
            Rgba([0, 0, 0, 0])  // transparent
        }
    })
}

// 13-very-large-photo: upscale 05-mountain to 3600×2400 via bilinear
// — tests > 8 MP path + truly large photo regime
fn gen_very_large(src: &RgbaImage, sw: u32, sh: u32) -> RgbaImage {
    imageops::resize(src, sw, sh, imageops::FilterType::Triangle)
}

// 14-soft-transparent: 800×600 with smooth alpha gradient + colorful photo
// underneath. Tests tier-2 (partial transparent) for non-synthetic content.
fn gen_soft_transparent(w: u32, h: u32, photo: &RgbaImage) -> RgbaImage {
    let pw = photo.width();
    let ph = photo.height();
    ImageBuffer::from_fn(w, h, |x, y| {
        let px = (x as u64 * pw as u64 / w as u64) as u32;
        let py = (y as u64 * ph as u64 / h as u64) as u32;
        let mut p = *photo.get_pixel(px.min(pw-1), py.min(ph-1));
        // alpha = smooth gradient from full opaque on left to ~50% on right
        let a = 255 - ((x * 128 / w.max(1)) as u8);
        p.0[3] = a;
        p
    })
}

// 15-monochrome-text: simulated grayscale text on white (extreme tier-3 case)
fn gen_mono_text(w: u32, h: u32) -> RgbaImage {
    let mut img = ImageBuffer::from_fn(w, h, |_, _| Rgba([255, 255, 255, 255]));
    // text-like: short horizontal strokes
    let mut rng = 0xDEADu64;
    for y_row in (50..h-50).step_by(40) {
        for x in 50..w-50 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            if (rng >> 60) & 0xF == 0 { continue; }  // skip spaces randomly
            for dy in 0..16 {
                if y_row + dy >= h { break; }
                let intensity = if dy < 2 || dy > 13 { 200 } else { 30 };
                *img.get_pixel_mut(x, y_row + dy) = Rgba([intensity, intensity, intensity, 255]);
            }
        }
    }
    img
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let out_dir = root.join("assets/png-bench/inputs-ext");
    std::fs::create_dir_all(&out_dir)?;
    let p05 = ImageReader::open(root.join("assets/png-bench/inputs/05-photo-mountain.png"))?
        .with_guessed_format()?.decode()?.to_rgba8();
    let p04 = ImageReader::open(root.join("assets/png-bench/inputs/04-photo-portrait.png"))?
        .with_guessed_format()?.decode()?.to_rgba8();

    let fixtures: [(&str, RgbaImage); 8] = [
        ("08-gradient-large.png", gen_gradient_large(2400, 1600)),
        ("09-ui-checker-text.png", gen_ui_checker(1920, 1080)),
        ("10-comic-flat.png", gen_comic_flat(1200, 900)),
        ("11-photo-noisy.png", gen_photo_noisy(&p05)),
        ("12-tiny-icon.png", gen_tiny_icon()),
        ("13-very-large-photo.png", gen_very_large(&p05, 3600, 2400)),
        ("14-soft-transparent.png", gen_soft_transparent(800, 600, &p04)),
        ("15-mono-text.png", gen_mono_text(1024, 768)),
    ];

    for (name, img) in &fixtures {
        let path = out_dir.join(name);
        img.save(&path)?;
        let (w, h) = (img.width(), img.height());
        let size = std::fs::metadata(&path)?.len();
        println!("wrote {} ({} x {} = {:.2} MP, {} KB)",
            name, w, h, (w as f64 * h as f64) / 1_000_000.0, size / 1024);
    }
    Ok(())
}
