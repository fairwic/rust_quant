# PA Quant Tree A/B/C v7 机制诊断报告

## 最终结论

```text
decision=archive_pa_standalone
```

本次唯一预注册诊断已经把问题拆清楚：v5 确认条件有一定事后筛选价值，但筛选后的 B 仍是负期望；等到确认信息真正可用并在 `t+2` 执行时，延迟入场又损失了平均 `0.337R`，使 C 比 B 明显更差。因此 PA 独立趋势策略失败的核心不是“还差一个阈值”，而是当前 setup 的原始 edge 不足，确认带来的选择改善无法覆盖执行延迟。

PA 独立策略到此归档。停止新候选、浅回踩、阈值网格和独立执行器开发；不打开密封 OOS，不进入 Shadow/Paper，不接 Vegas Meta-filter。资源重新集中到已有生产证据的 Market Velocity / Vegas。

## 证据身份

- 协议：`pa-diagnostic-v7-abc-counterfactual`
- 实验：`pa-v7-abc-counterfactual-once`
- 单次有效运行：退出码 0；没有第二次诊断。
- Git HEAD：`d502eca668a84c4e1b8efb38fdb131234c3c5ba6`
- PA scoped source fingerprint：`sha256:ae87ec802d43947551387a548f6c540a0e2b94b1183b50221e305a00dba8adfe`
- scoped dirty：`true`，如实表明结果来自当前未提交研究工作区。
- 联合数据指纹：`sha256:a94427809907a45402b4dde29c25f7811c35793a52661d4b911152606686adf4`
- K 线指纹：`sha256:2104caaef43352418afb8d94cd658e2959cde75ecdcdee1d61b241bac193f533`
- Funding 指纹：`sha256:54ba0abaac9f5772e7b3d9fbd4a9d4568906dde51c065a4f16a146e26a17069f`
- 公共训练窗口：`1752544800000` 至 `1784079000000`
- 每市场 K 线：35,039 根 15m 已确认 K 线
- 单次 CLI 运行的决策级聚合转录：[pa_quant_tree_abc_v7_result.json](evidence/pa_quant_tree_abc_v7_result.json)。该文件保留全部门禁、身份和汇总指标，但不是被终端截断前 stdout 的逐字节原件；`artifact_kind` 已如实标记这一边界。

本结果仍是已查看训练窗口的机制诊断，不具备外层 OOS 或生产晋级含义。

## A/B/C 结果

| 路径 | 样本 | 平均 R | 两倍成本平均 R | 胜率 | PF | 解释 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| A：原始 `t+1` 可执行基线 | 2,350 | -0.274 | -0.542 | 39.57% | 0.644 | 原始 setup 没有正 edge |
| B：确认筛选、复用 A 路径 | 255 | -0.077 | -0.429 | 50.20% | 0.882 | 事后筛选有改善，但仍不可交易且仍为负 |
| C：确认后 `t+2` 可执行路径 | 255 | -0.414 | -0.672 | 33.33% | 0.500 | 延迟追价后显著恶化 |

关键增量：

- B 相对 A 全集的描述性选择差异：`+0.197R`。
- 同 setup 严格配对 `C - B`：`-0.337R`。
- 两倍成本下 `C - B`：`-0.242R`。
- B 已固定为 `tradable=false`、`diagnostic_only=true`，没有被误包装成执行路径。

这组结果回答了 v6 留下的机制问题：确认规则不是完全没有信息，但它识别的是入场后才可见的扩张，真正等到可执行时，价格已经走到更差的位置。继续微调确认棒强度、ATR 上限或概率阈值，只会在同一个负 edge 上选择样本，不能修复时间可执行性。

## 分市场一致性

| 市场 | 严格配对 | B 平均 R | C 平均 R | C 胜率 | C PF | `C-B` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| BTC | 64 | -0.085 | -0.264 | 43.75% | 0.662 | -0.178 |
| ETH | 48 | -0.190 | -0.416 | 33.33% | 0.498 | -0.226 |
| SOL | 78 | -0.060 | -0.379 | 32.05% | 0.527 | -0.319 |
| BCH | 65 | -0.007 | -0.603 | 24.62% | 0.338 | -0.596 |

四个市场的 C 平均 R 全部为负，`C-B` 也全部为负。失败不是某个币种拖累，也没有任何正向市场可供“币种专用优化”辩解。

## 稳健性与统计门禁

- 7 日共享市场块 bootstrap 均值：`-0.414R`
- 单侧 95% 下界：`-0.604R`
- 删除最大五笔盈利后平均 R：`-0.451R`
- 共享组合：100.00U → 82.00U
- 共享组合最大回撤：`18.01%`
- C 正期望单侧 p：原始 `1.0`，Holm `1.0`
- 延迟保持单侧 p：原始 `1.0`，Holm `1.0`

通过的只有：严格配对 255 笔、覆盖四市场，以及 B 的诊断标志正确。失败的十项硬门禁为：

1. C 胜率不高于 60%；
2. C PF 不高于 1.2；
3. 基础成本平均 R 不为正；
4. 两倍成本平均 R 不为正；
5. bootstrap 下界不为正；
6. Holm 调整 p 值大于 0.05；
7. 删除最大五笔盈利后平均 R 不为正；
8. 没有三个“正平均 R 且至少 30 笔”的市场；
9. 组合最大回撤不低于 15%；
10. `C-B` 延迟增量为负。

## 工程基线修复

- 补齐 Range Breakout Drop 长期 EMA 字段在旧 tests/examples 中的构造，保持过滤器默认关闭，不改变旧实验语义。
- 把陈旧真实数据测试迁移到当前公开回测接口，并只接受 `QUANT_CORE_DATABASE_URL`，避免误连 `quant_web`。
- `rust-quant-strategies` 全部 tests/examples 已能完整编译。
- SMC/Keltner research executor 已从默认注册集合移除；显式研究注册仍保留。
- 新实验账本会拒绝重复/缺失身份、非法 p 值和未预注册的可晋级实验，并统一回写 Holm 调整值。
- 新报告绑定 Git HEAD、目标源码指纹和 scoped dirty 状态，不再使用含糊的 `working-tree`。

## 归档边界

保留：

- PA v1-v7 源码、测试、历史报告、数据指纹和只读复现能力；
- Feature Registry 与评估基础设施，作为审计资产；
- 本次 A/B/C 机制结论。

停止：

- PA 独立新策略、新候选和阈值扫描；
- 浅回踩 v6 或 followthrough 变体扩展；
- PA 独立 Shadow/Paper/Live 路径；
- 在当前已查看窗口继续挖掘 symbol 专用规则；
- 当前阶段的 Vegas PA Meta-filter 接入。

## 下一步：Market Velocity / Vegas

接下来不开发新策略家族，只做已有生产证据链维护：

1. 核对生产 Market Velocity 与 Vegas 的真实运行角色、镜像 revision、调度周期和数据新鲜度。
2. 逐层核对信号 → readiness → execution task → worker lease → signed read-only preflight → 保护单计划 → 订单结果回写。
3. 把阻塞原因、时间戳、交易对覆盖、费用/滑点口径和风控证据补齐到 operator-grade 诊断面。
4. 优先修复真实运行缺口与回归测试；不以新增指标、扩网格或新策略替代生产证据维护。

## 验证

- `cargo test -p rust-quant-analytics pa_quant_tree --lib -- --nocapture`：28/28 通过。
- `cargo test -p rust-quant-cli --bin pa_quant_tree_15m_research -- --nocapture`：7/7 通过。
- `cargo test -p rust-quant-strategies --no-run`：通过，全部 tests/examples 生成可执行文件。
- 唯一 A/B/C 命令：退出码 0，输出 `decision=archive_pa_standalone`。
