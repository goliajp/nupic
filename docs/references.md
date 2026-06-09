# 参考资料

> 按"我们站在谁的肩膀上"梳理。所有内容仅做**学习吸收**，不引入代码。

## PNG 格式本身

- **RFC 2083** — PNG 格式规范（核心，必读）
- **PNG Spec 2nd Edition** (W3C) — APNG 等扩展
- **Filter spec** — 5 个 filter 的精确定义和顺序

## DEFLATE / zlib 系

- **RFC 1951** — DEFLATE 格式规范
- **RFC 1950** — zlib 容器
- **zopfli** (Google, 2013, Apache-2.0) — DEFLATE 全局最优搜索的参考实现，思想必读
  - paper: "Data Compression with Arithmetic Encoding" 不是 zopfli 的，但 zopfli 的代码注释和 design doc 是最好的入门
- **libdeflate** (Eric Biggers, MIT) — DEFLATE 的 SIMD 加速实现，速度参考
- **igzip** (Intel) — AVX-512 DEFLATE，最快的实现

## PNG 优化工具（用于学习思想，不引入）

- **pngquant** / **libimagequant** (Kornel Lesiński, GPL-3) — 调色板量化
  - 核心：Wu's quantizer + median cut + Voronoi 迭代
- **oxipng** (Joshua Holmer, MIT) — 已是 Rust，filter + DEFLATE 优化
- **pngwolf** — filter 选择的早期研究
- **ECT** (fhanau, Apache-2.0) — 比 oxipng+zopfli 略好的 C++ 实现

## 颜色空间

- **OKLab** (Björn Ottosson, 2020, public domain) — 现代感知均匀色彩空间
  - blog post: bottosson.github.io/posts/oklab/
  - 数学定义清晰，参考实现就 40 行
- **OKLCh** — OKLab 的极坐标变体
- **ICtCp** (Dolby, 2015, Rec. 2100) — HDR 友好的感知色空间
- **JzAzBz** (Safdar 2017) — 另一个现代感知色空间
- **CIE 2000 ΔE 公式** — 比 Lab L2 更接近人眼但仍不如 SSIMULACRA2

## 感知 Metric

- **SSIMULACRA2** (Jon Sneyers / Cloudinary, 2023) — 当前 lossless/near-lossless 评测金标
  - paper: cloudinary.com/blog/introducing-ssimulacra
  - github: github.com/cloudinary/ssimulacra2（C++ 参考实现）
- **Butteraugli** (Google, 2018, Apache-2.0) — Google 系感知 metric
- **DSSIM** — SSIM 的 Lab 空间变体
- **VMAF** (Netflix) — 主要用于视频，但思路可借鉴

## 量化算法

- **Heckbert 1982** — Median Cut, pngquant 的祖宗
- **Wu 1992** — 主分量量化，更现代
- **k-means++** (Arthur & Vassilvitskii 2007) — 初始化策略
- **DPSO** — Discrete Particle Swarm Optimization 用于色彩量化
- **可微分量化** (多篇 2023-2024) — 关键词：differentiable quantization, learned palette, straight-through estimator
  - "Learning Compressed Image Representations" 等

## Dither

- **Floyd & Steinberg 1976** — 经典 error diffusion
- **Ulichney 1993** — Void-and-Cluster blue-noise mask 生成
- **Heitz 2019** — "A Low-Discrepancy Sampler that Distributes Monte Carlo Errors as a Blue Noise in Screen Space"（思想可迁移到 dither）
- **Riemersma dither** — Hilbert 曲线 error diffusion

## SIMD / 性能

- **Hacker's Delight** (Henry Warren) — 位运算技巧
- **Agner Fog's optimization guides** — 微架构级优化
- **`std::simd`** (Rust portable SIMD) — 跨平台 SIMD
- **`wide`** crate 思想（不引入，借鉴 API 设计）

## 数据集（benchmark 用）

| 数据集 | 用途 | 规模 |
|---|---|---|
| **Kodak True Color** | 经典彩色照片 baseline | 24 张 |
| **RAISE** (Dang-Nguyen 2015) | 高分辨率 RAW 转 PNG | 8156 张 |
| **CLIC** | Compression Lossy/Lossless Challenge 官方集 | 数千张 |
| **ClipArt-1k** | 矢量/合成图（filter 极敏感） | 1000 张 |
| **UI Screenshots** | 自采集，桌面/移动 UI 截图 | 待建 |
| **Subset of OpenImages** | 自然图像多样性 | 自选 |

## 现代 codec 对照（PNG pipeline 完成后的下一站参考）

- **AVIF** (AV1 Image File Format) — `libavif` + `rav1e` 思想
- **JPEG XL** — `libjxl` 思想，Cloudinary 团队主导，含 SSIMULACRA2 起源
- **WebP Lossless** — Google，2011，比 PNG 小约 25%

## 论文检索关键词

- "perceptually lossless image compression"
- "palette image compression"
- "predictive coding for indexed color"
- "blue noise dithering"
- "rate-distortion optimization PNG"
- "differentiable image quantization"
- "learned image compression near-lossless"
