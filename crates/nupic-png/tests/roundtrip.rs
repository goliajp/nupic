//! End-to-end roundtrip tests: encode_indexed_png → image crate decode
//! → byte-exact match.

use image::ImageReader;
use nupic_png::{IndexedImage, encode_indexed_png};
use rgb::Rgb;
use std::io::Cursor;

fn decode_via_image_crate(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let r = ImageReader::new(Cursor::new(png))
        .with_guessed_format()
        .unwrap()
        .decode()
        .expect("decode");
    let rgba = r.to_rgba8();
    (rgba.width(), rgba.height(), rgba.into_raw())
}

fn make_palette(rgb_triples: &[(u8, u8, u8)]) -> Vec<Rgb<u8>> {
    rgb_triples
        .iter()
        .map(|&(r, g, b)| Rgb { r, g, b })
        .collect()
}

#[test]
fn tiny_2x2_roundtrips() {
    let img = IndexedImage {
        width: 2,
        height: 2,
        palette: make_palette(&[(0, 0, 0), (255, 255, 255)]),
        indices: vec![0, 1, 1, 0],
        trns: None,
    };
    let png = encode_indexed_png(&img);
    let (w, h, rgba) = decode_via_image_crate(&png);
    assert_eq!((w, h), (2, 2));
    // Row 0: black, white → 0,0,0,255 | 255,255,255,255
    // Row 1: white, black → 255,255,255,255 | 0,0,0,255
    assert_eq!(rgba[0..4], [0, 0, 0, 255]);
    assert_eq!(rgba[4..8], [255, 255, 255, 255]);
    assert_eq!(rgba[8..12], [255, 255, 255, 255]);
    assert_eq!(rgba[12..16], [0, 0, 0, 255]);
}

#[test]
fn solid_color_image_roundtrips() {
    let img = IndexedImage {
        width: 16,
        height: 16,
        palette: make_palette(&[(128, 64, 192)]),
        indices: vec![0; 16 * 16],
        trns: None,
    };
    let png = encode_indexed_png(&img);
    let (w, h, rgba) = decode_via_image_crate(&png);
    assert_eq!((w, h), (16, 16));
    for px in rgba.chunks_exact(4) {
        assert_eq!(px, &[128, 64, 192, 255]);
    }
}

#[test]
fn gradient_roundtrips() {
    // 256 × 1 gradient — indices 0..255, palette is identity grayscale.
    let palette: Vec<Rgb<u8>> = (0..=255u8)
        .map(|v| Rgb { r: v, g: v, b: v })
        .collect();
    let indices: Vec<u8> = (0..=255u8).collect();
    let img = IndexedImage {
        width: 256,
        height: 1,
        palette,
        indices,
        trns: None,
    };
    let png = encode_indexed_png(&img);
    let (w, h, rgba) = decode_via_image_crate(&png);
    assert_eq!((w, h), (256, 1));
    for (i, px) in rgba.chunks_exact(4).enumerate() {
        let v = i as u8;
        assert_eq!(px, &[v, v, v, 255]);
    }
}

#[test]
fn trns_chunk_carries_alpha() {
    let img = IndexedImage {
        width: 2,
        height: 1,
        palette: make_palette(&[(0, 0, 0), (255, 0, 0)]),
        indices: vec![0, 1],
        trns: Some(vec![0, 128]), // palette[0] fully transparent, palette[1] 50%
    };
    let png = encode_indexed_png(&img);
    let (w, h, rgba) = decode_via_image_crate(&png);
    assert_eq!((w, h), (2, 1));
    assert_eq!(rgba[0..4], [0, 0, 0, 0]);
    assert_eq!(rgba[4..8], [255, 0, 0, 128]);
}

#[test]
fn large_random_indexed_roundtrips() {
    // 64 × 64 random indices over a 16-entry palette.
    let palette: Vec<Rgb<u8>> = (0..16u8)
        .map(|i| Rgb { r: i * 16, g: 255 - i * 16, b: (u32::from(i) * 32 % 256) as u8 })
        .collect();
    let mut s = 0xDEAD_BEEFu64;
    let indices: Vec<u8> = (0..64 * 64)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 32) % 16) as u8
        })
        .collect();
    let img = IndexedImage {
        width: 64,
        height: 64,
        palette: palette.clone(),
        indices: indices.clone(),
        trns: None,
    };
    let png = encode_indexed_png(&img);
    let (w, h, rgba) = decode_via_image_crate(&png);
    assert_eq!((w, h), (64, 64));
    for (i, px) in rgba.chunks_exact(4).enumerate() {
        let idx = indices[i] as usize;
        assert_eq!(px[0], palette[idx].r);
        assert_eq!(px[1], palette[idx].g);
        assert_eq!(px[2], palette[idx].b);
        assert_eq!(px[3], 255);
    }
}

#[test]
fn png_signature_and_chunks_present() {
    let img = IndexedImage {
        width: 1,
        height: 1,
        palette: make_palette(&[(0, 0, 0)]),
        indices: vec![0],
        trns: None,
    };
    let png = encode_indexed_png(&img);
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    // Chunks: IHDR, PLTE, IDAT, IEND
    let mut chunks_seen: Vec<[u8; 4]> = Vec::new();
    let mut p = 8;
    while p + 12 <= png.len() {
        let len = u32::from_be_bytes(png[p..p + 4].try_into().unwrap()) as usize;
        let mut ty = [0u8; 4];
        ty.copy_from_slice(&png[p + 4..p + 8]);
        chunks_seen.push(ty);
        p += 8 + len + 4;
        if &ty == b"IEND" {
            break;
        }
    }
    assert_eq!(chunks_seen.len(), 4);
    assert_eq!(&chunks_seen[0], b"IHDR");
    assert_eq!(&chunks_seen[1], b"PLTE");
    assert_eq!(&chunks_seen[2], b"IDAT");
    assert_eq!(&chunks_seen[3], b"IEND");
}
