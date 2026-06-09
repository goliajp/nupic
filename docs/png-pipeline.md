# PNG 编码 pipeline 的数学松弛分析

> 目标：找出 PNG 编码这条流水线上，**当前 SOTA（pngquant + oxipng + zopfli + ECT）相对于数学/感知上界还差多少**，以及差距分布在哪里。

## Pipeline 总览

PNG 编码 = 三段流水线 + 一个格式封套：

```
input image
   │
   ▼
[1] 颜色空间 / 位深变换       ◀──── 量化在这里 (if indexed PNG)
   │                                  ↑
   │                              最大数学松弛
   ▼
[2] 滤波 (filter, per scanline)
   │                                  ↑
   │                              中等数学松弛
   ▼
[3] DEFLATE (LZ77 + Huffman)
   │                                  ↑
   │                              ≈ 物理墙
   ▼
PNG chunks (IHDR / PLTE / IDAT / ...)
                                      ↑
                                  微小松弛
```

下面按从下往上的顺序展开（因为依赖关系是自下而上）。

---

## 第三段：DEFLATE —— 这是真·物理墙

### 当前 SOTA

- **zopfli** (Google, 2013) — 在 DEFLATE 格式约束下做近似全局最优搜索，对 backward references + Huffman 树同时优化
- **libdeflate** — 比 zopfli 快几十倍，压缩率略差（< 1%）
- **ECT** — 在 zopfli 基础上再加一些技巧，多数情况持平或略胜

### 数学分析

DEFLATE 格式的香农下界已经被 zopfli 近似达到。在 LZ77 + 静态/动态 Huffman 这套约束下，理论极限和实际产物的差距 < 0.5%。

**这里没有数学松弛**。再投入研究只能拿到工程层面的速度提升，不能拿到压缩率提升。

### 我们的选择

**写一份 zopfli 等价物**（压缩率持平 zopfli），但在**速度上拉开**：

- SIMD 加速 LZ77 字典搜索（AVX2 / NEON / wasm-simd128）
- 多线程并行（块间并行 + 块内候选并行）
- 现代缓存友好的数据结构

**预期目标**：同压缩率下，比 zopfli 快 50-100x；比 libdeflate 快 2-5x 同时压缩率追平 zopfli。

### 不做

- 尝试"超越 DEFLATE 的压缩率"——格式锁死的事，做了也没用
- 现代熵编码（ANS、算术编码）——一旦用就不是 PNG 了，这放到后续 pipeline

---

## 第二段：Filter —— 1-3% 的真松弛，被所有现有工具忽视

### PNG filter 机制回顾

每行 5 选 1：
- `None` — 原始像素
- `Sub` — 减去左像素
- `Up` — 减去上像素
- `Average` — 减去 (左 + 上) / 2
- `Paeth` — 减去 Paeth 预测器

filter 选择不改变像素信息，只改变 DEFLATE 看到的字节流。

### 当前 SOTA 做法

oxipng / pngwolf / advpng 全部用**贪心**：每行单独尝试 5 个 filter，选 deflate 后字节最少的。

### 数学分析

这是局部最优，不是全局最优。

**原因**：filter[i] 的最优选择依赖 filter[i-1], filter[i-2], ..., filter[i-32k] 留下的 LZ77 窗口状态。具体地：
- LZ77 是基于历史匹配的，filter[i] 输出与历史字节流的相似度影响压缩率
- 贪心忽略了 filter 选择之间的耦合
- 真正最优是 5^N 组合空间（N = 行数）

### 可以做的搜索方法

1. **DP + Beam Search**
   - 状态：(行号, LZ77 窗口指纹)
   - beam width B = 16~64
   - 复杂度：O(N · B · 5)
   - 期望：拿回大部分非贪心收益

2. **熵模型耦合**
   - 不用 deflate 实际字节数做评分（贵）
   - 用基于 LZ77 当前字典状态的条件熵估计
   - 快几个数量级，足够 driving 搜索

3. **多分辨率 rollback**
   - 先粗粒度（每 16 行选一次）贪心
   - 再在局部窗口内 simulated annealing 精修

### 数学上界

文献和经验估计：**比 oxipng 再省 1-3%**。
- 自然图像：1% 左右
- 合成图像 / UI 截图 / clipart：可达 3%（filter 选择对它们影响更大）

代价：10x ~ 50x 计算量（视搜索深度）。

---

## 第一段：颜色空间 + 量化 + Dither —— 最大数学松弛在这里

### pngquant / imagequant 的当前算法栈

每一项都比 2024-2026 SOTA 落后 10-50 年：

| 组件 | pngquant 当前实现 | 年份 | 数学/感知 SOTA | SOTA 年份 |
|---|---|---|---|---|
| 量化算法 | median cut + k-means 微调 | Heckbert 1982 | k-means++ / DPSO / 可微分 codebook | 2007 / 2018 / 2023+ |
| 颜色空间 | CIELab (1976) | 1976 | OKLab (Ottosson) / ICtCp / JzAzBz | 2020 / 2015 / 2017 |
| 感知 loss | Lab 欧氏距离 | 1976 | SSIMULACRA2 / Butteraugli | 2023 / 2018 |
| Dither | Floyd-Steinberg | 1976 | Blue-noise mask (void-and-cluster) / 模式调制 dither | 1993+ |

### 为什么这里能赢

**CIELab 的失败模式**（OKLab 论文有详细数据）：
- 饱和蓝紫色区域感知扭曲明显（"blue distortion"）
- 暗部均匀色调感知压缩
- 渐变中出现非线性"色阶感"

**OKLab 同样代价下**在这些病灶区域有显著改善。换言之，pngquant 在 Lab 下"最小化欧氏距离"找出的调色板，**不是人眼最小化感知误差的调色板**。

**SSIMULACRA2 vs L2-on-Lab 作为 loss function**：
- L2-on-Lab 假设 "颜色误差就是数学距离误差"
- SSIMULACRA2 综合了结构相似度、纹理对齐、注意力加权
- 在同 colors 数量下，用 SSIMULACRA2 做 quantizer 内层 loss 搜出来的 palette，对同一图客观感知质量直接高一档

### 三个可做的层级

#### Layer A：替换颜色空间和 metric（最便宜的大胜）

- 把 imagequant 流程里所有 Lab 用 OKLab 替换
- 把 Lab L2 用 SSIMULACRA2 替换
- 算法骨架不变（median cut / k-means）

**预期**：在大多数图像上 SSIMULACRA2 分数稳定高于 pngquant 0.5-1.5 分（满分 100），文件大小持平或微降。
**代价**：搜索成本 5-20x（SSIMULACRA2 比 L2 贵），但仍在工程可接受范围。

#### Layer B：升级 dither

- void-and-cluster blue-noise mask 替换 Floyd-Steinberg
- 视觉上肉眼可见提升（FS 的扫描线噪声消失）
- 文件大小持平或微降（blue-noise 频谱对 DEFLATE 友好度比 FS 差一点点，看具体图）

#### Layer C：可微分量化（研究性最后一公里）

- 把 palette 表示为可学习参数 `P ∈ R^{K×3}`（K 色，OKLab 三通道）
- 把 dither pattern 也表示为可学习参数
- Loss = SSIMULACRA2(decode(quantize(image, P, D)), image)
- 用 straight-through estimator 或 Gumbel-softmax 处理量化的不可导
- 梯度下降优化 P 和 D

**预期**：再压一层 1-2% 的天花板。
**风险**：研究性，工程稳定性差，超参敏感。

### 数学上界粗估

合并 A + B + C 后，相对 pngquant 的提升：
- 同感知质量下文件大小：节省 5-10%
- 同文件大小下感知质量（SSIMULACRA2）：高 2-4 分

这是有 paper 数据支撑的、可复现的边界。

---

## 格式封套层 —— 0.1-1% 的微小松弛

PNG chunks 本身有几个被无视的位：
- `iCCP` / `sRGB` chunk 的排布顺序
- `IDAT` chunk 的切分策略影响 streaming 解码 vs 压缩率
- ancillary chunk 的智能剔除（保留语义必需，删除浪费空间的）
- **APNG 单帧模式**有时比 PNG 静帧更小（几乎没人注意）

合计 0.1-1%，工程上顺手做。不是研究重点。

---

## 总松弛预算（粗估）

| 来源 | 节省范围 |
|---|---|
| Filter beam search（vs oxipng 贪心） | 1-3% |
| 颜色空间 + metric 升级（Layer A） | 2-5% |
| Dither 升级（Layer B） | 0-1%（主要是视觉质量，文件大小持平） |
| 可微分量化（Layer C） | 1-2% |
| 格式封套微调 | 0.1-1% |
| **合计** | **4-12%** 文件大小 |

加上"同文件大小下感知质量提升 2-4 分 SSIMULACRA2"。

这些数字不是营销，是基于已发表 paper 实验数据的合理上界估计。**真实实验出数前，所有数字都视为待验证假设**。

---

## 与 DEFLATE 物理墙的关系

注意：上述所有改进**都在 DEFLATE 之前**。一旦数据进 DEFLATE，剩下的压缩就是物理墙。我们的所有手段都是**让 DEFLATE 看到更短的输入**或**更少的高熵输入**，而不是让 DEFLATE 本身更厉害。

这也是为什么"PNG 这条 pipeline 终局是 4-12%"，再往后必须换格式（AVIF/JPEG XL）才能拿到 20-40% 的新一代收益。
