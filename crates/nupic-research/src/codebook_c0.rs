//! Stone C — `C0` baseline: differentiable soft-assignment codebook
//! learning with Adam, L2-in-OKLab loss surrogate, straight-through
//! at hard-quantise inference.
//!
//! Backs `docs/research/png/03c-bis-codebook-c0.md`. Goal: prove that
//! a metric-driven palette beats `imagequant` SSIMULACRA2 baseline on
//! 02-pluto (the only fixture stuck at SSIMULACRA2 -65 in cement).
//!
//! Algorithm (mini-batch flavour for fast iteration):
//!
//!   1. convert sRGB src to OKLab pixel array via nupic-color (Stone A)
//!   2. initialise palette with K=256 random pixel samples
//!   3. for each iter:
//!      a. sample a mini-batch of pixels (B << N)
//!      b. soft-assignment:
//!           weights[k] = softmax_k(-||x - palette[k]||² / τ)
//!      c. soft reconstruction:
//!           x̂ = Σ_k weights[k] * palette[k]
//!      d. L2 surrogate loss = ||x - x̂||² (in OKLab)
//!      e. gradient w.r.t. palette[k]:
//!           dL/dpalette[k] = -2 * weights[k] * (x - x̂)
//!         (closed-form for the soft-mean step; ignores ∂w/∂palette
//!         contribution which is small near convergence — Bengio 2013
//!         straight-through-ish simplification)
//!      f. Adam step
//!      g. anneal τ
//!   4. inference: hard argmin assignment of each pixel to its nearest
//!      palette entry, write indexed PNG
//!
//! Mini-batch size B = 4096 by default. n_iters = 500 default.

use nupic_color::{Oklab, srgb_u8_to_oklab};
use rgb::Rgb;

/// How the palette is initialised before Adam refinement.
#[derive(Clone, Copy, Debug)]
pub enum InitKind {
    /// Random pixel-sample init (simplest, may need many iters to converge).
    RandomSample,
    /// Initialise from imagequant's median-cut palette (Stone C as a
    /// **refinement** on top of cement). Recommended default — protects
    /// the photographic fixtures where imagequant already scores high.
    Imagequant,
}

/// Training hyper-parameters. Defaults mirror the 03c essay §6 spec.
#[derive(Clone, Copy, Debug)]
pub struct TrainConfig {
    pub n_colors: usize,
    pub n_iters: usize,
    pub batch_size: usize,
    pub temperature_start: f32,
    pub temperature_end: f32,
    pub learning_rate: f32,
    pub seed: u64,
    pub init: InitKind,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self {
            n_colors: 256,
            n_iters: 500,
            batch_size: 4096,
            temperature_start: 0.05,
            temperature_end: 0.005,
            learning_rate: 0.005,
            seed: 0xC0DE,
            init: InitKind::Imagequant,
        }
    }
}

pub struct Palette {
    pub colors_oklab: Vec<Oklab>,
}

/// Train a perceptually-uniform palette via Adam + soft-assignment.
/// Returns the learned palette in OKLab space.
pub fn train_palette_c0(
    src_rgba: &[u8],
    width: usize,
    height: usize,
    cfg: TrainConfig,
) -> Palette {
    let n_pixels = width * height;
    assert_eq!(src_rgba.len(), n_pixels * 4);

    // 1. convert sRGB → OKLab (Stone A)
    let mut pixels: Vec<Oklab> = Vec::with_capacity(n_pixels);
    for chunk in src_rgba.chunks_exact(4) {
        pixels.push(srgb_u8_to_oklab(Rgb { r: chunk[0], g: chunk[1], b: chunk[2] }));
    }

    // 2. init palette
    let mut rng = LcgRng::new(cfg.seed);
    let mut palette: Vec<Oklab> = match cfg.init {
        InitKind::RandomSample => {
            (0..cfg.n_colors)
                .map(|_| pixels[rng.next_below(n_pixels as u64) as usize])
                .collect()
        }
        InitKind::Imagequant => init_from_imagequant(src_rgba, width, height, cfg.n_colors),
    };

    // 3. Adam state — flat layout (3 channels × K, stored as 3 arrays).
    let k = cfg.n_colors;
    let mut m_l = vec![0f32; k]; let mut m_a = vec![0f32; k]; let mut m_b = vec![0f32; k];
    let mut v_l = vec![0f32; k]; let mut v_a = vec![0f32; k]; let mut v_b = vec![0f32; k];

    let beta1 = 0.9f32; let beta2 = 0.999f32; let eps = 1e-8f32;
    let inv_log_n = 1.0 / (cfg.n_iters as f32).max(1.0).ln();
    // For temperature: log-linear anneal from temperature_start to temperature_end.
    let log_t0 = cfg.temperature_start.ln();
    let log_t1 = cfg.temperature_end.ln();

    let mut batch_idx = Vec::with_capacity(cfg.batch_size);

    for iter in 0..cfg.n_iters {
        // anneal temperature log-linearly
        let frac = iter as f32 / (cfg.n_iters - 1).max(1) as f32;
        let log_tau = log_t0 * (1.0 - frac) + log_t1 * frac;
        let tau = log_tau.exp().max(1e-6);
        let inv_tau = 1.0 / tau;

        // sample mini-batch (replacement OK for simplicity)
        batch_idx.clear();
        for _ in 0..cfg.batch_size {
            batch_idx.push(rng.next_below(n_pixels as u64) as usize);
        }

        // accumulate gradient
        let mut grad_l = vec![0f32; k];
        let mut grad_a = vec![0f32; k];
        let mut grad_b = vec![0f32; k];

        for &p_idx in &batch_idx {
            let p = pixels[p_idx];
            // soft weights
            let mut w = vec![0f32; k];
            let mut max_neg = f32::NEG_INFINITY;
            for j in 0..k {
                let dl = p.l - palette[j].l;
                let da = p.a - palette[j].a;
                let db = p.b - palette[j].b;
                let d2 = dl * dl + da * da + db * db;
                let z = -d2 * inv_tau;
                w[j] = z;
                if z > max_neg { max_neg = z; }
            }
            // softmax with max-subtraction (numerical stability)
            let mut sum = 0f32;
            for j in 0..k {
                w[j] = (w[j] - max_neg).exp();
                sum += w[j];
            }
            let inv_sum = 1.0 / sum;
            for j in 0..k {
                w[j] *= inv_sum;
            }
            // reconstruction
            let mut rl = 0f32; let mut ra = 0f32; let mut rb = 0f32;
            for j in 0..k {
                rl += w[j] * palette[j].l;
                ra += w[j] * palette[j].a;
                rb += w[j] * palette[j].b;
            }
            // grad: dL/dpalette[j] = -2 · w[j] · (p - x̂)
            let err_l = p.l - rl;
            let err_a = p.a - ra;
            let err_b = p.b - rb;
            let two = 2f32;
            for j in 0..k {
                grad_l[j] -= two * w[j] * err_l;
                grad_a[j] -= two * w[j] * err_a;
                grad_b[j] -= two * w[j] * err_b;
            }
        }

        // average gradient over batch
        let inv_b = 1.0 / cfg.batch_size as f32;
        for j in 0..k {
            grad_l[j] *= inv_b;
            grad_a[j] *= inv_b;
            grad_b[j] *= inv_b;
        }

        // Adam step
        let t = (iter + 1) as i32;
        let one_minus_beta1_t = 1.0 - beta1.powi(t);
        let one_minus_beta2_t = 1.0 - beta2.powi(t);
        let lr = cfg.learning_rate;

        for j in 0..k {
            m_l[j] = beta1 * m_l[j] + (1.0 - beta1) * grad_l[j];
            m_a[j] = beta1 * m_a[j] + (1.0 - beta1) * grad_a[j];
            m_b[j] = beta1 * m_b[j] + (1.0 - beta1) * grad_b[j];
            v_l[j] = beta2 * v_l[j] + (1.0 - beta2) * grad_l[j] * grad_l[j];
            v_a[j] = beta2 * v_a[j] + (1.0 - beta2) * grad_a[j] * grad_a[j];
            v_b[j] = beta2 * v_b[j] + (1.0 - beta2) * grad_b[j] * grad_b[j];
            let mhat_l = m_l[j] / one_minus_beta1_t;
            let mhat_a = m_a[j] / one_minus_beta1_t;
            let mhat_b = m_b[j] / one_minus_beta1_t;
            let vhat_l = v_l[j] / one_minus_beta2_t;
            let vhat_a = v_a[j] / one_minus_beta2_t;
            let vhat_b = v_b[j] / one_minus_beta2_t;
            palette[j].l -= lr * mhat_l / (vhat_l.sqrt() + eps);
            palette[j].a -= lr * mhat_a / (vhat_a.sqrt() + eps);
            palette[j].b -= lr * mhat_b / (vhat_b.sqrt() + eps);
        }

        // silence unused warning
        let _ = inv_log_n;
    }

    Palette { colors_oklab: palette }
}

/// Initialise palette from imagequant's median-cut output, converted
/// into OKLab. Stone C acts as a **refinement** rather than a from-scratch
/// learner; this protects fixtures where imagequant already scores high.
fn init_from_imagequant(src_rgba: &[u8], width: usize, height: usize, n_colors: usize) -> Vec<Oklab> {
    fn try_palette(src_rgba: &[u8], width: usize, height: usize, q_min: u8) -> Result<Vec<rgb::RGBA8>, ()> {
        let pixels: Vec<rgb::RGBA8> = src_rgba.chunks_exact(4)
            .map(|c| rgb::RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
            .collect();
        let mut attrs = imagequant::new();
        attrs.set_quality(q_min, 95).map_err(|_| ())?;
        attrs.set_speed(4).map_err(|_| ())?;
        let mut img = attrs.new_image(pixels.as_slice(), width, height, 0.0).map_err(|_| ())?;
        let mut quant = attrs.quantize(&mut img).map_err(|_| ())?;
        let _ = quant.set_dithering_level(0.0);
        let (palette, _idx) = quant.remapped(&mut img).map_err(|_| ())?;
        Ok(palette)
    }

    let palette_rgba = try_palette(src_rgba, width, height, 70)
        .or_else(|_| try_palette(src_rgba, width, height, 0))
        .expect("imagequant init failed at q_min=0 too");
    let mut out: Vec<Oklab> = palette_rgba.iter()
        .map(|c| srgb_u8_to_oklab(rgb::Rgb { r: c.r, g: c.g, b: c.b }))
        .collect();
    if out.len() > n_colors { out.truncate(n_colors); }
    while out.len() < n_colors { out.push(out[0]); }
    out
}

/// Hard-quantise inference: for each pixel find nearest palette entry
/// (argmin L2 in OKLab) and return the index buffer + palette in sRGB
/// (so caller can encode as indexed PNG).
pub fn apply_palette(
    src_rgba: &[u8],
    width: usize,
    height: usize,
    palette: &Palette,
) -> (Vec<u8>, Vec<rgb::Rgb<u8>>) {
    let n_pixels = width * height;
    assert_eq!(src_rgba.len(), n_pixels * 4);
    let k = palette.colors_oklab.len();
    let mut indices = vec![0u8; n_pixels];
    for i in 0..n_pixels {
        let off = i * 4;
        let p = srgb_u8_to_oklab(Rgb { r: src_rgba[off], g: src_rgba[off + 1], b: src_rgba[off + 2] });
        let mut best_j = 0usize;
        let mut best_d2 = f32::INFINITY;
        for j in 0..k {
            let pj = palette.colors_oklab[j];
            let dl = p.l - pj.l; let da = p.a - pj.a; let db = p.b - pj.b;
            let d2 = dl * dl + da * da + db * db;
            if d2 < best_d2 {
                best_d2 = d2;
                best_j = j;
            }
        }
        indices[i] = best_j as u8;
    }
    // Convert palette to sRGB u8
    let mut palette_srgb = Vec::with_capacity(k);
    for c in &palette.colors_oklab {
        palette_srgb.push(nupic_color::oklab_to_srgb_u8(*c));
    }
    (indices, palette_srgb)
}

// --- LCG RNG (deterministic; not crypto-grade) ---

struct LcgRng { state: u64 }

impl LcgRng {
    fn new(seed: u64) -> Self { Self { state: seed.wrapping_mul(0x9E3779B97F4A7C15) | 1 } }
    fn next_u64(&mut self) -> u64 {
        // MMIX (Knuth) constants — adequate for sampling
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }
    fn next_below(&mut self, max: u64) -> u64 {
        (self.next_u64() >> 32) % max
    }
}
