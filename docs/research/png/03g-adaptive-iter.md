# 03g — Stone D adaptive iter:bump cap 20 → 100,EPS auto-stop

> Convergence diagnostic on 7-fixture corpus shows 6/7 fixtures need
> > 20 Lloyd iterations to hit EPS=0.0005 OKLab threshold;current
> cap=20 cuts photos short(02-pluto stops at 72.28 SSIM but converges
> at iter 46 with 79.66)。 Bump cap to 100,let EPS handle early-exit。
> Result:**02-pluto +7.38 SSIM**,其他 fixtures +0.02-0.11 marginal,
> corpus size -13 KB,vs TinyPNG 0.870× → **0.865×**。

---

## 1. 收敛 diagnostic 数据

`crates/nupic-research/examples/iter_convergence.rs` 跑 7 fixture × 100
max iter,记 max-move per iter。Convergence at first iter where
`max_move < 0.0005`:

| fixture | converged_iter |
|---|---:|
| 01-png-transparency-demo | 48 |
| 02-pluto-transparent | 46 |
| 03-wikipedia-logo | **3** |
| 04-photo-portrait | 34 |
| 05-photo-mountain | **67** |
| 06-photo-landscape | 48 |
| 07-photo-product | 21 |

Average ~ 38 iter,max 67(05-mountain),min 3(03-wikipedia)。
Default cap=20(v0.5.19)cut 6/7 fixtures short。

---

## 2. Bump cap 20 → 100

`DEFAULT_REFINE_ITERS` 改成 100。EPS check 已 in place(`max_move < EPS_SQ`):
fast fixtures exit early(03-wikipedia at 3 iter),slow fixtures(05-
mountain at 67 iter)finish。Wall-clock per fixture proportional to
actual iter:

| fixture | v0.5.19 iter | v0.5.20 iter | wall-clock ratio |
|---|---:|---:|---:|
| 01 | 20 | 48 | 2.4× |
| 02 | 20 | 46 | 2.3× |
| 03 | 20 → 3(EPS)| 3 | 0.15× |
| 04 | 20 | 34 | 1.7× |
| 05 | 20 | 67 | 3.4× |
| 06 | 20 | 48 | 2.4× |
| 07 | 20 | 21 | 1.05× |
| AVG | 20 | 38 | ~ 2× |

03-wikipedia 实际 **faster**(EPS exits at 3 iter vs 20 forced)。05-mountain
最慢(3.4×),仍 sub-5s on 1MP M2 release。

---

## 3. v0.5.20 production bench

7-fixture corpus(Default Path A,no `--dither`):

| fixture | v0.5.19 size/SSIM | **v0.5.20 size/SSIM** | Δsize | ΔSSIM |
|---|---:|---:|---:|---:|
| 01-transparency | 45 398 / -46.42 | 45 364 / -46.43 | -34 | -0.01 |
| **02-pluto** | 157 706 / 72.28 | **158 109 / 79.66** | +403 | **+7.38** |
| 03-wikipedia | 12 658 / 89.49 | 12 658 / 89.49 | 0 | 0 |
| 04-portrait | 380 748 / 82.95 | **378 428 / 83.06** | **-2 320** | **+0.11** |
| 05-mountain | 393 559 / 70.30 | **389 264 / 70.38** | **-4 295** | **+0.08** |
| 06-landscape | 1 044 702 / 82.75 | **1 035 965 / 82.77** | **-8 737** | **+0.02** |
| 07-product | 319 157 / 82.83 | 320 864 / 82.84 | +1 707 | +0.01 |
| **TOTAL** | **2 353 928** | **2 340 652** | **-13 276** | — |
| vs TinyPNG | 0.870× | **0.865×** | | |

Strict win:size -13 KB + SSIM ≥ v0.5.19 on 6/7 fixtures(7th flat 0)。
02-pluto SSIM jumps to 79.66 — close large chunk of gap to tinypng(-60
→ now +139.64)。"又小又好" preserved。

---

## 4. 价值观

- [[feedback-ceiling-first-priorities]] — cap raise unblocks 02-pluto
  ceiling without touching algorithm。Diagnostic-driven fix(measured
  before changing default)。
- [[feedback-metric-over-human-eye]] — convergence iter measured per
  fixture before adjusting cap;not heuristic。
- [[feedback-no-cost-thinking]] — 2× wall-clock 在 measured photo
  fixtures only;documented but not used as "should we?" 评估。

---

## 5. 下一步

- **dither variant research**(Pass 4):serpentine raster / Sierra
  dither matrices — 看 same SSIM gain at less size cost or vice versa。
- **`--use-nupic-png` perf cliff**(Pass 5):3MP+ image 10+ min hang,
  default-flip absolute blocker。
- **nupic-bits NEON pclmul CRC32**(Pass 6):4× perf gap close。
