# Cycle 118 — P-10 AVIF rescue wire — table 收尾报告

**Date**: 2026-06-19
**Verdict**: **GREEN, v1.2.11 SHIPPED**
**Spike**: `crates/nupic-research/examples/cycle118_avif_for_r6.rs`
**Wire**: `crates/nupic-cli/src/{cli,runner}.rs` 加 `--photo-rescue-avif` flag(与 `--photo-rescue-webp` 互斥)
**Data**: `assets/png-bench/cycle118/avif_sweep.{tsv,log}`

## AVIF vs WebP vs TinyPNG(同 6 张 R6 cohort)

| fixture | tiny_KB | WebP_KB(C116) | AVIF_KB(C118) | AVIF vs WebP | AVIF vs tiny |
|---|---:|---:|---:|---:|---:|
| p115_1024x768 | 200.0 | 17.3 | **14.7** | **-15%** | **0.074×** |
| p125_1920x1080 | 466.7 | 46.4 | **35.1** | **-24%** | **0.075×** |
| p167_1920x1080 | 442.0 | 51.3 | **26.0** | **-49%** | **0.059×** |
| p175_1920x1080 | 511.0 | 37.5 | **29.4** | **-22%** | **0.058×** |
| p214_2400x1600 | 1072.3 | 102.0 | **86.8** | **-15%** | **0.081×** |
| p274_3840x2560 | 2443.8 | 187.9 | **152.3** | **-19%** | **0.062×** |
| **mean** | — | — | — | **-24%** | **0.068×** |

**AVIF mean 0.068× tiny = 14.7× smaller than TinyPNG PNG**(WebP 11×)。

## DSSIM comparison(AVIF 5/6 strictly 优于 WebP)

| fixture | tiny_DSSIM | WebP_DSSIM | AVIF_DSSIM | AVIF vs WebP | AVIF vs tiny |
|---|---:|---:|---:|---:|---:|
| p115 | 0.001970 | 0.001373 | **0.001057** | **better** | -0.000913 |
| p125 | 0.009766 | 0.007535 | **0.005865** | **better** | -0.003901 |
| p167 | 0.000880 | 0.000699 | 0.000824 | +0.000125(微差仍 PASS)| -0.000056 |
| p175 | 0.001966 | 0.001389 | **0.001172** | **better** | -0.000794 |
| p214 | 0.002845 | 0.001680 | **0.001004** | **better** | -0.001841 |
| p274 | 0.003084 | 0.001463 | **0.000903** | **better** | -0.002181 |

**AVIF 比 WebP 在 5/6 fixture 上 DSSIM 更好**(p167 微差 +0.000125 仍 PASS strict tiny gate)。

## v1.2.11 wire 验证

| gate | result | OK |
|---|---|:---:|
| nupic --version | 1.2.11 | ✓ |
| baseline-7 default(no flag,must match v1.2.10 byte-identical)| 0.799× cohort | ✓ |
| 219 workspace tests | 219 pass 0 fail | ✓ |
| R6 cohort with `--photo-rescue-avif` | 6/6 AVIF output 15-156 KB | ✓ |
| `--photo-rescue-webp + --photo-rescue-avif` mutex | clap error "cannot be used with" | ✓ |
| extension swap `.png → .avif` | confirmed | ✓ |

## Trigger 设计(共享 P-09 predicate)

```
trigger = (args.photo_rescue_webp || args.photo_rescue_avif)
       && format == Png
       && output != stdout
       && (n_pixels >= 500_000 && opaque_fraction >= 0.95)

action:
- photo_rescue_avif:format → Avif,ext → .avif,default q=70
- photo_rescue_webp:format → Webp,ext → .webp,default q=80
```

Mutex by clap `conflicts_with`。

## Browser compatibility

| codec | Chrome | Safari | Firefox |
|---|---|---|---|
| WebP | 23+ | 14+ | 65+ |
| AVIF | 85+ | 16+ | 93+ |

WebP 兼容性广,AVIF 在现代 audience 优势。User 选 flag。

## Workflow speed

| spike | jobs | wall |
|---|---:|---:|
| cycle118_avif_for_r6 | 18(6 fixture × 3 q)| **7.9s** ✓ |

(spike 用 sips 解 AVIF + DSSIM;production 不用 sips,client-side decode)

## Algorithm-ideas board 更新

| 候选 | status |
|---|---|
| **J. 2-pass K-up fail-safe** | SHIPPED v1.2.9 |
| **WebP transcoder(P-09)** | SHIPPED v1.2.10 |
| **AVIF transcoder(P-10)** | **SHIPPED v1.2.11 ✨** |
| C. slow-tier zopfli flag | open(Cycle 119 候选)|
| `.nupic` container | retired |
| paper writeup | optional research |

## Cycle 119+ next-up

- **C. slow-tier `--effort 9` zopfli flag**(opt-in,1 cycle)
- 或 JPEG transcoder(类似 wire,但 JPEG 在 photo 上不如 WebP/AVIF — 低优先)
- 或 paper writeup(optional)

推荐 Cycle 119 = C slow-tier zopfli(完成 transcoder + slow-tier 系列)。
