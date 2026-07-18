# PA Quant Tree v1 实现设计

## 目标

新增无 AI 运行时的 PA 量化策略能力，支持趋势回撤、区间边界回归、冻结过滤模型、Vegas Shadow Meta-filter，以及离线 Champion/Challenger 研究流程。

## 运行时合同

- 所有结构判断只读取当前及更早的已确认 K 线。
- PA 候选在信号棒收盘后生成，最早于下一根 K 线开盘执行。
- 运行时模型只读取不可变 Manifest，不读取滚动胜率或在线训练状态。
- Vegas Shadow Meta-filter 不修改原始方向、价格、止损或目标。
- 无法形式化的结构返回 `NO_TRADE` 与稳定 blocker code。

## v1 模块

### 策略模块

- `features`：EMA、ATR、效率比、重叠率、方向化 K 线几何等确定性特征。
- `candidate`：趋势回撤与区间边界候选。
- `model`：规则、逻辑回归、CART 和小型森林的冻结推理表示；CART 运行时深度上限 6、叶子数上限 64。
- `manifest`：Feature Registry、受限 DSL、规范化序列化与 hash。
- `strategy`：状态化下一棒执行适配器。
- `vegas_shadow`：只读 Meta-filter 对照。

`PaQuantTreeStrategy` 当前是 manifest 驱动的 `IndicatorStrategyBacktest` 构造入口，尚未注册到默认 `StrategyExecutor` 表。现有注册表没有可审计的 manifest 解析、Champion 指针或 Paper 状态来源；在这些 contract 完成前，禁止把 `pa_range_15m`、`pa_range_1h`、`pa_trend_15m`、`pa_trend_1h` 或 `vegas_pa_meta_filter` 加入默认生产执行列表。

### 研究模块

- 时间顺序切分与 purge；若训练样本结算时间抵达验证信号时点则明确拒绝切分。
- 成本后 R 指标、最大回撤、Profit Factor 和配对路径增量。
- M0-M4 模型竞赛：无过滤、固定 PA 规则、L2 逻辑回归、CART 与小型树集成。每个 walk-forward 折只用训练段拟合参数，再用验证段评分；one-standard-error 规则优先简单模型。
- Champion/Challenger 状态机和晋级门禁。
- 共享组合回放与证据 manifest：风险预算、同币种单持仓、同刻同比缩放、组合回撤、代码/数据/实验身份。
- 共享市场时间块 bootstrap 与 Holm-Bonferroni 校正；同一时间块的多币种结果必须整体重采样。
- 数据准备度 Gate：独立 PA 策略必须同时有 BTC、ETH、其他币种各至少 1,000 根目标周期确认 K 线及公共时间窗口。

## 明确不包含

- 实时 AI 分析。
- 在线修改 Champion。
- 自动 Live promote。
- 限价/突破挂单生命周期。
- 未经验证的楔形、MTR、三角形检测。
- 生产数据库迁移、部署和真实交易。

## 验收标准

- 相同 K 线和 Manifest 输出完全一致。
- 追加未来 K 线不改变历史时点决策。
- Vegas Shadow 过滤不会修改原始 SignalResult。
- DSL 拒绝未知特征、越界参数和未来引用。
- Challenger 不允许跳过 `validated -> shadow -> paper_challenger` 状态。
- 本轮基础设施不等于统计优势：必须将真实、成本后、时间点一致的候选结果接入 `ResearchDataset`，执行密封 OOS 和至少 90 天/100 笔 Forward Paper 后，才可产生 PromotionRecord。
- 已复用现有 backfill 补齐 BTC/ETH/SOL/BCH 的 15m 与 1h 全年历史：15m 每市场 35,039 根，1h 每市场 8,759 根，公共窗口连续且缺口为 0。
- 15m 与 1h 已分别完成训练期研究并使用独立数据指纹；四个 v1 独立策略均为负期望，没有生成合格 Challenger，密封 OOS 仍未打开。
- 后续 v3 已补齐 Hyperliquid 单一来源的全年小时资金费率代理，并按绝对费率保守扣减；联合指纹绑定K线、funding与训练协议。该代理不等于OKX实际成本，仍禁止Promote。
- v4新增 Feature Registry v2 的趋势质量特征；15m M2验证均值转正但未通过标准误、两倍成本、胜率和跨市场门槛，因此只保留为弱信号研究证据。
- v5新增独立趋势跟随确认策略，严格按 setup、确认、下一棒开盘三个时点执行；15m四市场全部为负且组合回撤18.01%，1h仅97笔不足训练门槛，因此归档失败且不影响v1/v4。
- 评估框架v6为入选模型保存OOF决策，并只对OOF保留路径执行两倍成本、组合和block bootstrap；15m趋势M2的bootstrap下界为-0.238R，继续禁止晋级。legacy v4输出保持字节级一致。
- v7 是冻结协议后唯一一次 PA A/B/C 机制诊断：B 相对 A 显示 +0.197R 的描述性选择差，但可执行 C 相对 B 损失 -0.337R；C 的均值、两倍成本、胜率、PF、bootstrap、稳健性、跨市场和回撤门禁均失败，固定结论为 `archive_pa_standalone`。
- PA 独立策略、后续 PA 候选和 PA Meta-filter 研究均已停止；不进入 Shadow/Paper/Live，不打开密封 OOS。后续资源集中到已有生产证据的 Market Velocity/Vegas。
- analytics focused tests 通过；既有 `range_breakout_drop` 测试初始化和真实数据 smoke API 已修复，策略 crate 全目标构建基线恢复通过。
