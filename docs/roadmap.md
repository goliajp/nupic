# 构建路线图

> 自底向上的 8 阶段模块图 + 工程顺序建议。所有体量估计仅为粗略数量级，落地后实际值以代码为准。

## 模块依赖图

```
            ┌─────────────────────────────────┐
            │  阶段 8: Benchmark & Datasets   │
            └────────────────┬────────────────┘
                             │
            ┌────────────────┴────────────────┐
            │  阶段 7: Filter Beam Search     │
            └────────────────┬────────────────┘
                             │
       ┌─────────────────────┼────────────────────┐
       │                     │                    │
┌──────┴────────┐    ┌──────┴───────┐    ┌──────┴────────┐
│ 阶段 6:       │    │ 阶段 5:      │    │ 阶段 2:       │
│ Blue-noise    │    │ Quantizer    │    │ PNG container │
│ Dither        │    │ (k-means++   │    │ + filter naïve│
│               │    │ + diff)      │    │               │
└──────┬────────┘    └──────┬───────┘    └──────┬────────┘
       │                    │                   │
       └────────┬───────────┘                   │
                │                               │
       ┌────────┴──────────┐                    │
       │ 阶段 4:           │                    │
       │ SSIMULACRA2       │                    │
       └────────┬──────────┘                    │
                │                               │
       ┌────────┴──────────┐                    │
       │ 阶段 3:           │                    │
       │ OKLab / ICtCp 色  │                    │
       │ 彩管线 (SIMD)     │                    │
       └────────┬──────────┘                    │
                │                               │
                └───────────┬───────────────────┘
                            │
                ┌───────────┴────────────┐
                │ 阶段 1: DEFLATE        │
                │ encoder (zopfli-grade) │
                └───────────┬────────────┘
                            │
                ┌───────────┴────────────┐
                │ 阶段 0: 基础设施 (CRC, │
                │ Adler, bit I/O, SIMD)  │
                └────────────────────────┘
```

## 阶段详表

| # | 模块 | 思想来源 | 体量估计 | 输出 oracle |
|---|---|---|---|---|
| 0 | CRC32 + Adler32 + bit reader/writer (SIMD) | 标准定义 + zlib / libdeflate SIMD 套路 | ~500 行 | zlib 输出可校验 |
| 1 | **DEFLATE encoder**（zopfli 等价压缩率，SIMD 大幅加速） | zopfli 论文 + libdeflate SIMD 思想 | 3-5k 行 | zopfli 输出可对照 |
| 2 | PNG 容器 + filter 朴素实现（5 选 1 贪心） | RFC 2083 | 1-2k 行 | libpng round-trip 可校验 |
| 3 | OKLab / ICtCp / OKLCh 色彩管线（SIMD） | Björn Ottosson 2020 + Rec. 2100 | ~800 行 | 参考实现数值可对照 |
| 4 | **SSIMULACRA2** metric | Cloudinary 团队 paper + JXL 项目参考实现 | ~2k 行 | 参考实现分数可对照 |
| 5 | **量化器**：k-means++ → 进阶可微分 palette refinement | Arthur & Vassilvitskii 2007 + 2023+ 可微分量化 paper | 3-5k 行 | pngquant 输出做对照 baseline |
| 6 | Blue-noise dither (void-and-cluster) | Ulichney 1993 + Heitz 2019 | ~500 行 | 频谱分析可校验 |
| 7 | Filter beam search + 熵模型 | 自研，部分参考 pngwolf | 1-2k 行 | oxipng 输出做对照 baseline |
| 8 | Benchmark harness（数据集 + metric + 多 codec runner） | — | 1-2k 行 | — |

**总量粗估**：~13-20k 行 pure Rust。

每个阶段独立可测、独立可 benchmark。这是这个项目能跑下来的关键 —— 不是一个 13k 行的怪物 PR，是 8 个可独立交付的子项目。

---

## 工程顺序建议

### 三个起手选项

#### 选项 A —— 从地基开始（DEFLATE 优先）

顺序：阶段 0 → 1 → 2 → 8 → 3 → 4 → 5 → 6 → 7

**优点**：
- 所有上层都依赖 DEFLATE，先把它写完没有"返工"风险
- 有清晰 oracle（zopfli 输出）可对照
- 写完 DEFLATE 后就有"超 zopfli 50x 速度同压缩率"的硬指标可对外宣布

**缺点**：
- 前期 8-10 周看不到与"超越 pngquant"相关的进展
- DEFLATE + PNG 容器写完才能进 metric / quantizer，反馈周期长
- 容易在底层细节打磨太久，错过对量化层的早期 insight

#### 选项 B —— 从顶层 prototype 开始（量化器先出）

顺序：阶段 3 → 4 → 5 → 2(粗) → 8 → 6 → 1 → 7 → 0(精修)

**优点**：
- 最快交付"数学胜过 pngquant"的可演示成果（4-6 周）
- 早期就能发现量化器 / metric / 颜色空间相关的全部坑
- 用现成 DEFLATE（先借一个，最后再自研替换）跑通端到端管线

**缺点**：
- 违反"0 deps"宪法的临时妥协（中间状态会引一个 DEFLATE 依赖）
- 最后替换 DEFLATE 时可能要回头调上层接口
- "暂时引依赖"的状态如果项目放缓会变成永久状态（人性陷阱）

#### 选项 C —— 先做 benchmark harness（推荐）

顺序：阶段 8(数据集 + baseline 跑通) → 3 → 4 → 5 → 2(粗) → 6 → 7 → 1 → 0(精修)

**优点**：
- 立靶子：所有现有 SOTA 的真实数据先量出来，作为后续每一步改进的对照
- 立项后 1-2 周就有"客观 baseline 表格"可看，迭代有数据基线
- 后续每个改动都能"客观证明"自己确实推进了 SOTA，不靠口头评估
- "我们的目标是把这个数字从 X 推到 Y" 比 "我们要做更好的 PNG" 更具体

**缺点**：
- 第一周看起来什么都没产出（其实是产出了最重要的实验工具）
- harness 本身的 0-dep 化是个隐藏负担（要解析多种 codec 输出格式）

### 我的推荐

**选项 C → B 的混合 → 最后做 A**：

1. **第 1-2 周**：建 benchmark harness（阶段 8 的第一版）
   - 选定数据集（Kodak / RAISE / ClipArt / UI Screenshots 各几百张）
   - 选定 metric（SSIMULACRA2 + Butteraugli + 文件大小 + 编码时间）
   - 跑 pngquant / oxipng / ECT / cwebp lossless / avifenc lossless 全套，拿到 baseline 表格
   - 这一步**可以临时引外部 codec 作为黑盒** —— 因为 harness 调用的是 CLI，不是 link 的库，不污染我们的代码库

2. **第 3-8 周**：阶段 3 + 4 + 5（颜色管线 + SSIMULACRA2 + 量化器 prototype）
   - 阶段 2 同步做一个朴素 PNG 写出器（只支持 indexed，filter 全 None）
   - 用现成 DEFLATE（先借 system zlib / 任意 crate，**作为 ABI 黑盒**，不抄它的代码到我们仓库里）
   - 出第一个"数学胜过 pngquant"的演示版本
   - benchmark harness 跑出对照数据

3. **第 9-12 周**：阶段 6 + 7（dither 升级 + filter beam search）
   - 此时管线已稳，可以专心打磨细节
   - 同步开始阶段 1（DEFLATE）的设计 / 论文研读

4. **第 13-20 周**：阶段 1（DEFLATE 自研）
   - 此时上层稳定，DEFLATE 接口清晰
   - 写完后立刻替换掉临时依赖，**项目首次进入"完全 0 dep"状态**
   - 这一步是项目成立后的第一个里程碑庆功点

5. **第 20+ 周**：阶段 0 精修 + 阶段 5 可微分量化部分
   - 0 是基础设施回顾打磨
   - 5 的可微分部分是研究性，放到地基稳了再啃

**关键节点**：
- **Week 2 节点**：baseline 数据表出，项目可量化
- **Week 8 节点**：数学胜过 pngquant 的 demo 出，方向验证
- **Week 20 节点**：完全 0 dep，宪法首次完全满足

### 不推荐的反模式

- **不要一开始就追求 0 dep**：会在阶段 0/1 卡死，看不到任何端到端进展，士气崩
- **不要一开始就追求所有层最优**：先朴素跑通，再迭代深化
- **不要跳过 benchmark harness 直接开始优化**：没有数据对照，所有"我觉得变好了"都是自欺
- **不要在阶段 5 直接上可微分量化**：先把 k-means++ + OKLab + SSIMULACRA2 跑通，可微分是 cherry on top

---

## 命名 / 仓库结构（待落地时再敲定）

预备 Rust workspace 草图（不是承诺）：

```
nupic/
├── Cargo.toml                 # workspace
├── crates/
│   ├── nupic-bits/            # 阶段 0
│   ├── nupic-deflate/         # 阶段 1
│   ├── nupic-png/             # 阶段 2 + 7
│   ├── nupic-color/           # 阶段 3
│   ├── nupic-ssimulacra/      # 阶段 4
│   ├── nupic-quantize/        # 阶段 5 + 6
│   ├── nupic-cli/             # 命令行壳
│   └── nupic-bench/           # 阶段 8
└── datasets/                  # gitignored, 拉脚本管理
```

每个 crate 受 file-size.md 硬限约束：单文件 ≤ 500 行，单函数 ≤ 200 行。
