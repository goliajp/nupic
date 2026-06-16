//! Cycle 17 — sanity check var-diff sampling on synthetic large images.
//! Cycle 11's classify_for_auto_dither samples every 4th row when
//! n_total > 1M, and breaks early when count > 1M. For 4K-class images
//! (8 MP), this means only the top ~half is sampled. If image vertical
//! content distribution is non-uniform (e.g. sky on top, ground below),
//! classifier could mis-pick d strength.
//!
//! Validate by:
//! 1. tiling 05/06 to 2x2 = ~6 MP synthetic 4K
//! 2. comparing var-diff signal on tiled vs original
//! 3. checking d-pick consistency

use std::path::PathBuf;
use anyhow::Result;
use image::{ImageReader, RgbaImage};
use nupic_quantize::classify_for_auto_dither;

fn workspace_root() -> Result<PathBuf> {
    let m = env!("CARGO_MANIFEST_DIR");
    Ok(PathBuf::from(m).ancestors().nth(2).unwrap().to_path_buf())
}

fn tile_2x2(rgba: &RgbaImage) -> RgbaImage {
    let (w, h) = (rgba.width(), rgba.height());
    let (nw, nh) = (w * 2, h * 2);
    let mut out = RgbaImage::new(nw, nh);
    for y in 0..nh {
        for x in 0..nw {
            let px = rgba.get_pixel(x % w, y % h);
            out.put_pixel(x, y, *px);
        }
    }
    out
}

fn flip_vertical(rgba: &RgbaImage) -> RgbaImage {
    let (w, h) = (rgba.width(), rgba.height());
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(x, h - 1 - y, *rgba.get_pixel(x, y));
        }
    }
    out
}

fn classify(raw: &[u8], w: u32) -> f32 {
    classify_for_auto_dither(raw, w)
}

// Vertical concat:img_top (h rows) above img_bot (h rows), assuming
// equal width. Used to construct an adversarial sky-on-top +
// texture-below image and stress var-diff vertical sampling bias.
fn concat_vertical(top: &RgbaImage, bot: &RgbaImage) -> RgbaImage {
    assert_eq!(top.width(), bot.width());
    let w = top.width();
    let h_top = top.height();
    let h_bot = bot.height();
    let mut out = RgbaImage::new(w, h_top + h_bot);
    for y in 0..h_top {
        for x in 0..w {
            out.put_pixel(x, y, *top.get_pixel(x, y));
        }
    }
    for y in 0..h_bot {
        for x in 0..w {
            out.put_pixel(x, y + h_top, *bot.get_pixel(x, y));
        }
    }
    out
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    let fixtures = [
        "04-photo-portrait.png",
        "05-photo-mountain.png",
        "06-photo-landscape.png",
        "07-photo-product.png",
    ];

    println!("=== part 1: original + tile2x + flip ===");
    println!("{:<32} {:>10} {:>10} {:>10} {:>10}",
        "fixture", "w x h", "orig_d", "tile2x_d", "flip_d");
    for fname in &fixtures {
        let path = root.join("assets/png-bench/inputs").join(fname);
        let img = ImageReader::open(&path)?.with_guessed_format()?.decode()?;
        let orig = img.to_rgba8();
        let (w, h) = (orig.width(), orig.height());

        let d_orig = classify(&orig.as_raw(), w);
        let tiled = tile_2x2(&orig);
        let d_tile = classify(&tiled.as_raw(), tiled.width());
        let flipped = flip_vertical(&orig);
        let d_flip = classify(&flipped.as_raw(), w);

        println!("{:<32} {:>10} {:>10.2} {:>10.2} {:>10.2}",
            fname, format!("{}x{}", w, h), d_orig, d_tile, d_flip);
    }

    println!();
    println!("=== part 2: adversarial vertical concat (smooth top + textured bot) ===");
    // Construct: top half = 04 (portrait, smooth skin, var≈34)
    //            bot half = 06 (landscape, textured, var≈665)
    // Truth: this image has mixed content; ground-truth tier preference
    // unclear. But sampling bias means top-half sample dominates →
    // classifier sees var ≈ 34 → picks d=0.5. Lower half (textured) won't
    // get the d=0.7 it would prefer in isolation.
    let smooth_path = root.join("assets/png-bench/inputs/04-photo-portrait.png");
    let textured_path = root.join("assets/png-bench/inputs/06-photo-landscape.png");
    let smooth = ImageReader::open(&smooth_path)?.with_guessed_format()?.decode()?.to_rgba8();
    let textured_full = ImageReader::open(&textured_path)?.with_guessed_format()?.decode()?.to_rgba8();
    // Resize textured to match smooth width
    let textured = image::imageops::resize(
        &textured_full, smooth.width(), smooth.height(),
        image::imageops::FilterType::Nearest,
    );

    let smooth_top = concat_vertical(&smooth, &textured);
    let d_st = classify(&smooth_top.as_raw(), smooth_top.width());

    let textured_top = concat_vertical(&textured, &smooth);
    let d_tt = classify(&textured_top.as_raw(), textured_top.width());

    println!("{:<32} {:>10} {:>10}", "config", "w x h", "d");
    println!("{:<32} {:>10} {:>10.2}",
        "smooth_top+textured_bot",
        format!("{}x{}", smooth_top.width(), smooth_top.height()),
        d_st);
    println!("{:<32} {:>10} {:>10.2}",
        "textured_top+smooth_bot",
        format!("{}x{}", textured_top.width(), textured_top.height()),
        d_tt);

    if d_st != d_tt {
        println!();
        println!("!!! sampling bias detected: d depends on vertical orientation");
    } else {
        println!();
        println!("part 2: classifier robust to vertical content position");
    }

    println!();
    println!("=== part 3: 4-MP+ adversarial — does sampling reach bottom? ===");
    // Build 1200×3200 = 3.84 MP image. With cap=500K samples and
    // step=4, sampled_y_max = 500K / 1200 × 4 = 1666 → only top 52%
    // of rows touched. If bottom content matters, classifier misses it.
    // Make top = smooth (04 portrait, var≈34), bottom = textured (06).
    let textured_strip = image::imageops::resize(
        &textured_full, 1200, 800,
        image::imageops::FilterType::Nearest,
    );
    // 4 strips of 04 + 4 strips of textured = 8 × 800 = 6400 rows.
    // That's 7.68 MP. Cut to 4-strip variants:
    //   smooth4_then_textured4: top half all smooth, bot half textured
    //   textured4_then_smooth4: opposite
    let stripes_st = {
        let mut acc = smooth.clone();
        for _ in 0..3 { acc = concat_vertical(&acc, &smooth); }
        let mut t = textured_strip.clone();
        for _ in 0..3 { t = concat_vertical(&t, &textured_strip); }
        concat_vertical(&acc, &t)
    };
    let stripes_ts = {
        let mut t = textured_strip.clone();
        for _ in 0..3 { t = concat_vertical(&t, &textured_strip); }
        let mut acc = smooth.clone();
        for _ in 0..3 { acc = concat_vertical(&acc, &smooth); }
        concat_vertical(&t, &acc)
    };
    let d_4st = classify(&stripes_st.as_raw(), stripes_st.width());
    let d_4ts = classify(&stripes_ts.as_raw(), stripes_ts.width());
    println!("{:<40} {:>12} {:>10}", "config", "w x h", "d");
    println!("{:<40} {:>12} {:>10.2}",
        "stripes: smooth(top4) + textured(bot4)",
        format!("{}x{}", stripes_st.width(), stripes_st.height()), d_4st);
    println!("{:<40} {:>12} {:>10.2}",
        "stripes: textured(top4) + smooth(bot4)",
        format!("{}x{}", stripes_ts.width(), stripes_ts.height()), d_4ts);
    if d_4st != d_4ts {
        println!();
        println!("!!! 4-MP sampling BIAS confirmed:");
        println!("    smooth-top → d={:.2}, textured-top → d={:.2}", d_4st, d_4ts);
        println!("    classifier sees only top half of large images");
    } else {
        println!();
        println!("part 3: classifier robust even at 4-MP+ scale");
    }
    Ok(())
}
