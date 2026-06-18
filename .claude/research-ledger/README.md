# .claude/research-ledger

跨 cycle 累积的研究台账。每个 cycle 收尾时**必报、必增**三类内容:

| file | 内容 | 增长方式 |
|---|---|---|
| `cycle-XYZ-table-report.md` | 每 cycle 收尾的关键指标 table(per-fixture + decision gate matrix + 任何 bucket 切片) | 每 cycle 新建一文件,不覆盖旧的 |
| `paper-material.md` | 论文素材积累(reproducible findings、methodology、远景 thesis kernel) | 累积式,追加新条目,加 cycle 来源 tag |
| `algorithm-ideas.md` | 算法创新候选(每条带 evidence + 可行性 + 论文级 + 下一步) | 累积式,追加 / 升降级、cycle 间 cross-link |

跟 `memory/` 区分:
- `memory/` 是 **session 工作内存**(MEMORY.md / cycle-XYZ-kickoff.md / feedback-*.md),帮新 session 快速恢复上下文
- `.claude/research-ledger/` 是 **项目工件 ledger**(版本控制,跨 session 跨人累积),供任何人(包括未来论文写作)直接 cite

跟 `docs/research/png/04*.md` essay 区分:
- essay 是某一 cycle **完整 narrative**(背景 + spike + verdict + 数据 + 下一步),面向技术读者
- table report 是 essay 数据 table 的**抽取版**,面向"我要查 Cycle X 的数字"的快速 lookup
- paper material / algorithm ideas 是**跨 cycle 的 reduction**,essay 不存
