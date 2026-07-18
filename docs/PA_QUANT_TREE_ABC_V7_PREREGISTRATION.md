# PA Quant Tree A/B/C v7 机制诊断预注册

## 冻结身份

- 实验 ID：`pa-v7-abc-counterfactual-once`
- 协议：`pa-diagnostic-v7-abc-counterfactual`
- CLI 显式入口：`--timeframe 15m --candidate-family baseline --evaluation-protocol abc-counterfactual-v7`
- 状态：训练窗口机制诊断；不是密封 OOS、Shadow、Paper 或 Live。
- 执行次数：只允许生成一份有效 JSON 结果。若命令在读取/结算前因环境或工程错误退出，可修复工程后重试；一旦生成有效诊断 JSON，不得以任何理由调整参数后重跑。
- 原始证据：`docs/evidence/pa_quant_tree_abc_v7_result.json`，不得覆盖。

运行时必须记录当前 Git HEAD、PA 目标源码 SHA-256 指纹和 scoped dirty 状态。dirty 结果不会阻止本次训练诊断，但会永久写入证据，避免把工作区结果伪装成已提交 revision。

## 固定数据

- 市场：`BTC-USDT-SWAP`、`ETH-USDT-SWAP`、`SOL-USDT-SWAP`、`BCH-USDT-SWAP`。
- 周期：仅 15m。
- K 线：`quant_core` 中四市场各自最多 50,000 根已确认 K 线，截取四市场共同起止窗口。
- 当前窗口全部视为已经看过的训练数据；本实验不打开密封 OOS。
- 资金费率：Hyperliquid `fundingHistory` 的既有分源小时事实，按持仓覆盖小时累计绝对费率，作为保守跨交易所代理。
- 成本：单边 5bps 手续费 + 3bps 滑点；同时计算两倍全部成本压力。
- 任何数据缺口、非确认 K 线、时间倒序或资金费率小时缺口都会使命令失败，不允许按零补齐。

## 固定 A/B/C 语义

### A：原始可执行基线

- setup 在 `t` 收盘成立，只允许冻结的 `pa_trend_15m` TrendPullback 定义。
- 在 `t+1` 开盘按当时价格复核风险并入场。
- 使用 `t` 冻结的方向和结构止损。
- A 统计所有已结算的原始趋势 setup，不按未来确认筛选。

### B：确认筛选诊断

- 只保留 `t+1` 收盘通过冻结 v5 followthrough 确认规则的 setup。
- 完整复用 A 的 `t+1` 入场、目标、止损和退出结果。
- 固定输出 `tradable=false`、`diagnostic_only=true`。
- B 仅用于描述确认条件的事后选择价值，不能形成执行器或晋级证据。

### C：确认后的可执行路径

- 与 B 使用相同的已确认 setup 集合。
- 在 `t+2` 开盘按当时价格重新构造目标并复核风险后入场。
- 方向和结构止损仍冻结自 `t`；`t+2` 之后的数据只用于已入场路径结算。

B/C 配对 ID 固定为 `symbol + setup_ts + direction`。ID、市场、setup 时间、方向或结构止损不一致，或者任一路径未结算时，不得进入严格配对统计，并必须计入拒绝原因。

## 预注册假设与多重检验

本批次只有两个单侧假设，组成同一个 Holm-Bonferroni family：

1. `pa-v7-abc-c-expectancy`：严格配对 C 的基础成本平均 R 大于 0。
2. `pa-v7-abc-delay-preservation`：同 setup 的 `C - B` 基础成本平均增量大于或等于 0。

原始单侧 p 值使用固定种子 `20260715` 的七日共享市场块符号翻转检验：先按自然日保留同期多市场相关性，再从首日开始合并为连续七日块，对整个七日块执行 20,000 次符号翻转；使用加一修正。观察均值不大于 0 时 p 值固定为 1。两个原始 p 值统一执行 Holm-Bonferroni 校正，按原实验顺序写回账本。

此外，C 使用自然日共享市场块、连续块长 7、10,000 次重采样、固定种子 `20260715` 计算平均 R 的单侧 95% bootstrap 下界。

## 固定报告口径

- A、B、C 的基础成本与两倍成本样本数、平均 R、胜率、PF、总 R 和路径回撤。
- B 相对 A 全集的平均 R 选择差异；只作描述，不作为严格配对因果量。
- 严格配对 `C - B` 的基础成本和两倍成本平均增量。
- 每个市场的 A/B/C 指标、严格配对数和拒绝原因。
- C 的七日共享市场块 bootstrap 下界。
- 两个假设的原始单侧 p 值与 Holm 调整值。
- C 删除最大五笔盈利后的平均 R。
- C 按既有共享风险预算回放的组合最大回撤。

## 固定门禁

C 只有同时满足下列全部条件，才输出 `retain_for_future_validation`：

1. 严格配对至少 100 笔，且至少覆盖三个有配对市场；
2. 胜率严格大于 60%；
3. PF 严格大于 1.2；
4. 基础成本和两倍成本平均 R 均严格大于 0；
5. 七日共享市场块 bootstrap 单侧 95% 下界严格大于 0；
6. C 正期望假设的 Holm 调整单侧 p 值不大于 0.05；
7. 删除最大五笔盈利后平均 R 严格大于 0；
8. 至少三个市场平均 R 为正，且每个正向市场至少 30 笔；
9. 共享组合最大回撤严格低于 15%；
10. `C - B` 基础成本平均增量不小于 0；
11. 所有 B 路径均为 `tradable=false`、`diagnostic_only=true`。

任一条件失败，决策固定为 `archive_pa_standalone`。即使全部通过，本轮也不得打开密封 OOS、创建 Shadow/Paper 或修改生产 Vegas/Market Velocity；只允许保留为未来新数据验证假设。

## 失败后的资源方向

若归档 PA 独立策略：

- 停止 PA 新候选、浅回踩、阈值网格和独立执行器开发；
- 保留历史代码、数据、报告和只读复现入口；
- 从默认执行注册和活跃 backlog 中移除失败研究策略；
- 下一阶段只维护已有生产证据的 Market Velocity / Vegas：数据新鲜度、readiness、风控、执行证据与生产回归。
