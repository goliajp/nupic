# lab29-nupic

Pure-Rust, zero-dependency image codec research project. Stand on the shoulders of every great open-source codec (pngquant / oxipng / zopfli / mozjpeg / ravif / jxl), reimplement from scratch, push to the math/physics boundary.

## 项目宪法

1. **没有 ROI / win-risk 框架，只有数学与物理边界。** 决策依据是"信息论极限"、"感知模型极限"、"硬件极限"，不是"性价比"。
2. **0 deps，所有路径自研。** 不引外部 codec / 算法库；可以"读论文 + 读开源实现学思想"，但代码全部自己写。
3. **目标格式必须跨平台。** 浏览器 + iOS + Android + Win/Mac/Linux 原生可打开。如果做新格式，兼容性是硬约束。
4. **业务无关。** 不为任何具体业务调优；目标是 codec 本身的工艺顶点。
5. **一条 pipeline 做一条。** 不并行铺开。**第一条 pipeline 是 PNG**。
6. **2026/6 立项。** 不接受"几十年传统不可超越"的论证。后人就是要站在前人肩膀上。

## 仓库结构

```
lab29-nupic/
├── README.md              # 本文件
├── docs/
│   ├── requirements.md    # 项目宪法的来源（用户原话 + 解读）
│   ├── png-pipeline.md    # PNG 编码三段流水线的数学松弛分析
│   ├── roadmap.md         # 自底向上的 8 阶段构建图 + 工程顺序
│   └── references.md      # 关键 paper / 开源参考 / 数据集
└── (后续 Rust 工作区在 docs 落定后建)
```

## 当前阶段

设计期。代码 / Cargo workspace 尚未建立。

下一步决策点 —— 起手顺序（见 `docs/roadmap.md` 末尾）：

- A. 从 DEFLATE 写起（最底层 oracle，所有上层依赖它）
- B. 从 OKLab + SSIMULACRA2 + 量化器 prototype 写起（先做出能数学胜过 pngquant 的 demo，DEFLATE 临时复用，后回头自研）
- C. 先做 benchmark harness（选数据集、定 metric、跑现有 SOTA 拿全 baseline，后续每一步改进都有可信对照）

推荐顺序：**C → B → A**（先立靶子，再打靶心，最后打地基）。
