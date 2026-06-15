# 00 — PNG codec attack surface(v0.4.0 站点视角)

> Anchor 篇。把 [`docs/png-pipeline.md`](../../png-pipeline.md) 的理论上界分析,
> 跟 v0.4.0 上 [`assets/png-bench/baseline.json`](../../../assets/png-bench/baseline.json)
> 实测对照,排出值得攻击的点。**所有后续 `docs/research/png/01-...` 必须挂在
> 这张表的某一格上。**

---

## 0. 我以为的攻击点 — 数据翻盘了

研究面起手第一刀是看 02-pluto:nupic 158 KB / DSSIM 0.075,TinyPNG 180 KB
/ DSSIM 0.018。我们体积更小,**质量被甩 4×**。第一直觉:TinyPNG 用了
non-indexed PNG(RGB + tRNS),所以质量好。

实测打脸了:

```text
$ python3 docs/research/png/_chunk-table.py   # 7 张图全 chunk dump
```

每张图的 color_type / palette / tRNS / IDAT:

| 文件 | src | ct | palette | tRNS | IDAT |
|---|---|---|---:|---:|---:|
| 01-png-transparency-demo | inputs | RGBA | 0 | 0 | 224,509 |
| 01-png-transparency-demo | nupic-0.4 | idx | 256 | **256** | 52,803 |
| 01-png-transparency-demo | tinypng | idx | 256 | 247 | 47,199 |
| 02-pluto-transparent | inputs | RGBA | 0 | 0 | 469,797 |
| 02-pluto-transparent | nupic-0.4 | idx | 256 | 19 | **157,993** |
| 02-pluto-transparent | tinypng | idx | 256 | 22 | **179,917** |
| 03-wikipedia-logo | inputs | idx | 188 | 58 | 15,126 |
| 03-wikipedia-logo | nupic-0.4 | idx | **110** | 31 | 12,694 |
| 03-wikipedia-logo | tinypng | idx | 128 | 41 | 12,986 |
| 04-photo-portrait | inputs | RGB | 0 | 0 | 1,156,798 |
| 04-photo-portrait | nupic-0.4 | idx | **114** | 0 | 374,381 |
| 04-photo-portrait | tinypng | idx | **256** | 0 | 569,110 |
| 05-photo-mountain | inputs | RGB | 0 | 0 | 1,550,006 |
| 05-photo-mountain | nupic-0.4 | idx | 256 | 0 | **462,489** |
| 05-photo-mountain | tinypng | idx | 256 | 0 | **433,413** |
| 06-photo-landscape | inputs | RGB | 0 | 0 | 2,691,855 |
| 06-photo-landscape | nupic-0.4 | idx | 256 | 0 | 1,091,399 |
| 06-photo-landscape | tinypng | idx | 256 | 0 | 1,091,017 |
| 07-photo-product | inputs | RGB | 0 | 0 | 884,908 |
| 07-photo-product | nupic-0.4 | idx | **164** | 0 | 340,079 |
| 07-photo-product | tinypng | idx | **256** | 0 | 366,577 |

**两边都用 indexed PNG。两边都 256 color palette(大多数图)。** 攻击点
不在 "我们没做 stage 0 自适应",在 stage 1 内部。

这是研究面起手第一次 explore/verify 纠错:**chunk-level dump 是必须的,
不能停在 size / DSSIM 数字层**。

---

## 1. PNG 编码 7 道工序

[`docs/png-pipeline.md`](../../png-pipeline.md) 拆成 3 段(color/quant →
filter → DEFLATE)+ 1 个 chunk 封套。把 chunk 封套拆成 stage 0(color
type 选择)和 stage 4(chunk packaging),把 color/quant 拆成 stage 1a
(color space)和 1b(quantization),共 7 道:

```
input image
  │
  ▼
[0] color type / bit depth selection
  │     ← 实测两家都用 indexed,这里没攻击面
  ▼
[1a] color space transform (sRGB → working space)
  │     ← png-pipeline §1 Layer A:CIELab → OKLab
  ▼
[1b] palette quantization (if indexed)
  │     ← 真攻击面在这里:策略差异
  ▼
[2] per-row filter selection (None/Sub/Up/Avg/Paeth)
  │     ← png-pipeline §2:1-3% 松弛被 oxipng/pngquant 忽视
  ▼
[3] DEFLATE encode (LZ77 + Huffman)
  │     ← png-pipeline §3:zlib → zopfli 5-15%,zopfli → 上界 < 0.5%
  ▼
[4] chunk packaging (IHDR / PLTE / tRNS / IDAT / metadata strip)
  │     ← 微小松弛
  ▼
output PNG
```

---

## 2. 真攻击面 — stage 1b 内部的 quality–entropy trade-off

回到 chunk 表,聚焦 stage 1b(palette quantization)的三组反例:

### 反例 A:nupic 体积大幅胜,质量打平

**04-photo-portrait** — nupic 114 palette / 374 K / DSSIM 0.00157;TinyPNG
256 palette / 569 K / DSSIM 0.00162。

imagequant 在 `quality (70, 95)` 区间发现 114 colors 已经达到 95 目标,
停止扩 palette。TinyPNG 总是塞满 256,palette 表 + tRNS 都拉满,IDAT
entropy 高。**这是 imagequant 的 quality-driven palette sizing 的胜利**,
*不是* TinyPNG 浪费 — 大概率 TinyPNG 的策略文档默认就是 "fill 256"。

### 反例 B:nupic 体积小,质量大幅输

**02-pluto-transparent** — nupic 256 palette / 158 K / DSSIM 0.075;
TinyPNG 256 palette / 180 K / DSSIM 0.018。

两边都满 palette,差异是 **IDAT 22 KB**。这 22 KB 是 entropy 多出来的:
TinyPNG 选了更"散"的 palette + 更激进 dither,**牺牲 deflate 收益换取
DSSIM 改善**。nupic(imagequant default `set_dithering_level(1.0)`)选
deflate-friendly 路径,质量代价大。

### 反例 C:同 palette 大小,TinyPNG 体积稳赢

**05-photo-mountain** — nupic 256 palette / 462 K;TinyPNG 256 palette /
433 K。29 K 差。两边 alpha 都 0,差异纯在 IDAT。这次是 TinyPNG 选了
**更 deflate-friendly** 的 palette mapping;nupic 选了视觉略好但 entropy
略高的。

### 结论

nupic(imagequant default `quality (70, 95)` + dither 1.0)和 TinyPNG 是
**stage 1b 内部 quality–entropy trade-off 上的两个固定操作点**,各自
有强有弱,没有绝对优劣。

我们的真正缺陷:**这条 trade-off 曲线 nupic 用户看不到,也调不了**。
TinyPNG 商业产品看不到调节面,但内部大概率根据图自适应(05 vs 02 反向
策略)。

---

## 3. 七道工序的 v0.4.0 位置 + 距离

| stage | nupic 0.4.0 | SOTA | 物理 / 数学上界 | 距离 SOTA | 距离上界 |
|---|---|---|---|---|---|
| 0 — color type 选择 | 强制 indexed(Auto)/ 强制 RGB(Lossless) | TinyPNG 实测亦总是 indexed | per-image min over color types | **没差**(实测) | n/a |
| 1a — color space | sRGB / image crate identity | OKLab + perceptual metric(无产品落地)| OKLab + SSIMULACRA2 | 1 代 | 2 代 |
| 1b — quantization 策略 | imagequant default,quality (70, 95),dither 1.0,fixed | TinyPNG adaptive | Voronoi-optimal palette w.r.t. perceptual metric(NP-hard) | **per-image 各赢一些**(看 trade-off 点)| 远 |
| 1b — quantization 算法 | imagequant = median cut + k-means(Heckbert 1982) | k-means++ / DPSO / differentiable codebook | 同上 | 数十年 | 远 |
| 2 — filter selection | oxipng `--filters all`(per-row greedy) | 同 | NP-hard beam-search 全局最优 | 0 | 1-3% |
| 3 — DEFLATE | oxipng default → zlib level 9 | zopfli / libdeflate | Shannon entropy on LZ77 tokens | **5-15%**(没接 zopfli)| < 0.5% beyond zopfli |
| 4 — chunk packaging | oxipng `StripChunks::Safe` | 同 | 微小 | 0 | < 1% |

---

## 4. 攻击点 top-5(v0.5.x 调度)

| 序 | 攻击点 | 预计收益 | 难度 | 形态 | 关联 essay |
|---|---|---|---|---|---|
| 1 | **stage 1b 策略可调:把 dithering / quality range / palette-fill 策略暴露到 `Quality::Auto` 之外,默认按图自适应** | 修复 02-pluto / 05-mountain 这种 trade-off 错位;不破坏 04 的胜利 | low(改 `encode_png_lossy`,加 heuristic) | cement-layer(0.4.x) | 01-pluto-case(已计划) |
| 2 | **stage 3 接 zopfli** | 全 7 张 5-15% IDAT 节省;实测见 01 后再 calibrate | low(加 dep 或调 oxipng-zopfli) | cement-layer | 02-deflate(待写)|
| 3 | **stage 1a + 1b Layer A:SSIMULACRA2 + OKLab guided palette** | 同质量下 -2~-5% size,SSIMULACRA2 +0.5~1.5 分(`png-pipeline.md` §1)| **high**(需 SSIMULACRA2 stone,roadmap 阶段 4) | stone-layer(0.6.x+) | 03-perceptual(待写) |
| 4 | **stage 2 filter beam search** | 1-3% IDAT(`png-pipeline.md` §2) | high | stone-layer(roadmap 阶段 7) | 04-filter(待写) |
| 5 | ~~stage 0 自适应 color type~~ | **0**(实测 TinyPNG 也都用 indexed) | — | — | killed-by-data |

### 优先级 rationale

- 攻击 #1 是**唯一立竿见影修 02-pluto 的路径**,且不会回退 04 的胜利。
  trade-off 是策略问题不是算法问题 — 我们已经持有 imagequant + DSSIM 这
  两个工具,差的只是把它们组装成 "Auto 自动选 trade-off 点"。
- 攻击 #2 是工程红利,加 dep 就有收益,没有不做的理由。
- 攻击 #3-4 是 stone 层路线,跟 [`roadmap.md`](../../roadmap.md) 的 8 阶
  段衔接,长期上限。

---

## 5. Open questions(下一篇 01 来回答)

1. **dithering_level 在 02-pluto / 05-mountain 上的 trade-off 曲线**怎么
   长?把 imagequant `set_dithering_level` 从 0.0 扫到 1.0,
   (size, DSSIM)关系是单调还是有 sweet spot?
2. **quality range 怎么 driven by metric**?`Quality::Perceptual(Dssim, t)`
   今天是在 q 维度二分搜,**但 q=mid 时 imagequant 内部跑的还是固定
   range** — 应该让 metric 直接驱动 imagequant 的 set_quality 而不是
   两层包装。预计这一改本身就能消掉 02-pluto 的差距大半。
3. **TinyPNG 在 02 上用了什么 dither**?能否从 IDAT entropy 反推出
   dither 类型(Floyd-Steinberg vs blue-noise)?
4. **palette-fill 策略**:imagequant 让 quality 决定 palette size;
   TinyPNG 强制 256。我们的 "imagequant 114 colors → 0.66× TinyPNG" 优
   势能不能在所有 sparse 图上复现?

01 = 02-pluto case study,会回答 1-3。问题 4 留给 02-deflate 或独立小
实验。

---

## 6. 引用与材料

- [`docs/png-pipeline.md`](../../png-pipeline.md) — 整段 stage 1/2/3 理论
  上界分析的源
- [`docs/roadmap.md`](../../roadmap.md) — 8 阶段 stone-layer 路线
- [`assets/png-bench/baseline.json`](../../../assets/png-bench/baseline.json) — v0.4.0 实测 TinyPNG 对照
- [Kornel Lesiński, *libimagequant 4.x*](https://github.com/ImageOptim/libimagequant) — stage 1b 当前实现
- [Google, *zopfli* (Vandevenne et al., 2013)](https://github.com/google/zopfli) — stage 3 reference
- [Björn Ottosson, *A perceptual color space for image processing*](https://bottosson.github.io/posts/oklab/) — stage 1a SOTA
- [Jon Sneyers et al., *SSIMULACRA 2*](https://github.com/cloudinary/ssimulacra2) — stage 1a/1b perceptual metric SOTA

测量脚本(全 7 张图 chunk dump 表的来源):
[`docs/research/png/_chunk-table.py`](_chunk-table.py)
