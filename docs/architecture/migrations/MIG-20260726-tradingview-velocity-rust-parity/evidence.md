# Migration Evidence：MIG-20260726-tradingview-velocity-rust-parity

## 1. 身份

| 字段 | 值 |
| --- | --- |
| Migration program / Owner child | `MP-tradingview-velocity-research-characterization / MIG-20260726-tradingview-velocity-rust-parity / Research` |
| Manifest kind / Registry | `historical_record / docs/architecture/migrations/programs/registry.toml` |
| 可作为 future dependency | `false` |
| Manifest | `docs/architecture/migrations/MIG-20260726-tradingview-velocity-rust-parity/manifest.toml` |
| Manifest SHA-256 | `a7d5893912c157a217178ab9feb6d5569f4c6e067de270554759f37022c7dfb7`；历史原件未留存 hash |
| 被测试代码 commit | `660407f438db52acde8318e705b26ad05948aa0a` |
| 被测试范围补丁 SHA-256 | 历史未留存；本 Evidence 不能作为当前 HEAD Verdict |
| 架构基线 revision | `660407f438db52acde8318e705b26ad05948aa0a` |
| 迁移模式 | `behavior_change` |
| Evidence scope | `historical_characterization` |
| 技术 state | `verified`（仅已记录的 Research 回放） |
| Promotion status | `research_only` |
| Cutover status | `not_required` |
| Verification mode | `manual` |
| Evidence 生成时间 | `2026-07-26 Asia/Shanghai` |

### 1.1 输入、输出与可重放边界

| 类型 | ref | SHA-256 | 结论 |
| --- | --- | --- | --- |
| Strategy 行为输入 | `15min_velocity_all_symbol_strategy_research_66d3937e.pine` | `3d2b82f4297d9d4a661d8ca2c04daf8d0b1cebc8a1b62e63f60d923f3ccc7799` | 已固定 |
| Strategy 研究说明 | `15min_velocity_all_sybol_strategy.md` | `6ba36109ad9cdb005daec019dbd41c313e9f78fbf3290b2ed3e52e86393f95b4` | 已固定 |
| Market 历史输入 | TradingView/OKX 当时观察窗口 | 未保存 DatasetManifest/原始 K 线 hash | 只能作为 scenario comparison |
| 输出 | 当时人工指标与逐笔审计 | 未内容寻址 | 不能复用为当前 HEAD Verdict |

本记录故意不把 `evidence.md` 当作动态输入。未来的 `current_migration` 必须由 Market owner 先发布不可变 DatasetManifest，再由 Strategy owner 发布规则工件；Research 只读消费二者并保存有 hash 的输出工件。

## 2. 范围核对

- 唯一 Owner：Research；Market 历史数据与 Strategy 规则分别是外部输入 owner，不是本 Manifest 的 secondary owner。
- Source：冻结 Pine `66d3937e` 与中文策略规范。
- Target：独立 Rust Research 状态机和只读对照 CLI。
- Contract、数据库 Schema、生产入口、Paper/Live 与交易所 mutation：均不改变。
- 旧 Rust V1～V13：没有覆盖、删除或切换。

结论：本记录保存已发生的 Research scenario 对照，不构成可重复的目标目录迁移、Market 数据迁移、Strategy 发布迁移或生产策略晋级。

## 3. 迁移前基线

### 3.1 真实调用链

```text
Market historical observation（未冻结 DatasetManifest）
  -> Strategy rule artifact（冻结 Pine）
  -> Research Pine 指标与状态机
  -> 信号棒收盘提交订单
  -> 下一根开盘模拟成交
  -> TradingView broker emulator 保护单
  -> Strategy Tester
```

### 3.2 Characterization

- Pine 本地文件与 TradingView 编辑器：`1603` 行、JavaScript UTF-16 长度 `94515`、UTF-8 `101367` 字节、FNV-1a 32 `66d3937e`，完全一致。
- BTC 当前 Strategy Tester：7 笔，净利润 `265.00 USDT`，Gross Profit/Loss `1753.50 / 1488.50`，PF `1.1780315754`，最大回撤 `976.20`，未计佣金。
- ETH 当前 Strategy Tester：14 笔，净利润 `331.42 USDT`，Gross Profit/Loss `374.41 / 42.99`，PF `8.7092347057`，最大回撤 `22.06`，未计佣金。
- TradingView 研究约定：`volume` 直接映射 OKX `vol_ccy`，不使用 `volume * close`。

## 4. 实施结果

### 4.1 工件与职责

| 层 | 文件 | 职责 |
| --- | --- | --- |
| Research 入口 | `crates/rust-quant-cli/src/bin/tradingview_velocity_parity.rs` | 冻结结束时点、30/60/90天窗口、60天预热、零成本与成本压力 |
| 只读行情适配 | `crates/rust-quant-cli/src/app/tradingview_velocity_parity.rs` | OKX spot 15m `history-candles`、`vol_ccy` 映射、连续性与 Pine hash 校验 |
| Model | `.../tradingview_velocity_parity/model.rs` | 信号家族、订单意图、仓位、交易、阻断原因与指标 |
| Indicator | `.../tradingview_velocity_parity/indicators.rs` | Pine EMA/RMA/RSI/ATR/SMA、nearest-rank 与完整量能事件 |
| Structure | `.../tradingview_velocity_parity/ranges.rs` | 逆势横盘、20根确认箱体、大型水平箱体、上升三角 |
| Signal | `.../tradingview_velocity_parity/signals.rs` | 所有当前 Pine 入场家族、冲突、冷却、突破保护与状态冻结 |
| Broker | `.../tradingview_velocity_parity/engine.rs` | 下一根开盘、反手、保护单、动态保本、OHLC 路径与最大回撤 |

### 4.2 Pine 到 Rust 规则矩阵

| Pine 家族 | Rust `SignalFamily` | 结果 |
| --- | --- | --- |
| RSI 底/顶背离 | `RsiBullishDivergence` / `RsiBearishDivergence` | 已实现 |
| RSI 极值吞没/长影 | `RsiOversoldPattern` / `RsiOverboughtPattern` | 已实现 |
| EMA 趋势量能 | `EmaTrendLong` / `EmaTrendShort` | 已实现 |
| 20根确认箱体接受 V2 | `ConfirmedRangeAcceptanceLong` | 已实现 |
| 大型水平箱体突破 | `LargeHorizontalRangeBreakLong` | 已实现 |
| 大型上升三角突破 | `LargeAscendingTriangleBreakLong` | 已实现 |
| 20根突破失败做空 | `AnchorFalseBreakShort` | 已实现 |
| 多头切换后重复扫高 | `TransitionLiquiditySweepShort` | 已实现 |
| EMA 压缩后扩张 | `EmaCompressionExpansionLong` / `EmaCompressionExpansionShort` | 已实现 |
| 三棒强势反包做多 | `ThreeBarBullishEngulfingLong` | 已实现 |
| 普通超买空头保护 | `recent_bullish_transition` 与冻结突破线门禁 | 已实现 |
| 多空同棒冲突、同向不加仓、反向下一开盘反手 | `SignalState` + `Broker` | 已实现 |

MACD 与布林带不是独立入场家族：MACD 只展示，布林中轨只参与三棒反包确认。`2026-07-22` 的“量能高潮但价格未创新高”仍是未冻结的新假设，未混入本次 parity。

### 4.3 执行口径

- `barstate.isconfirmed` 后产生意图，`t+1` 开盘成交；不使用未来K线补造入场。
- 普通保护位冻结为 tick 距离并相对真实入场价计算；形态止损和结构目标冻结为绝对价格。
- 同棒触价按 TradingView 默认 broker path：开盘更靠近高点时 `O→H→L→C`，否则 `O→L→H→C`。
- 最大回撤使用 TradingView 公式：入场前已平仓权益峰值，加持仓期间沿可达路径产生的不利价格偏移。
- TradingView 零成本基线之外，独立报告每边 `5bp` 手续费加 `3bp` 滑点，不用成本结果反向修改信号。滑点当前作为平仓时扣减的等价成本，不移动棒内成交价，因此成本场景的棒内权益曲线是压力诊断，不是逐笔盘口仿真。

## 5. 验证结果

| 验证 | 命令/证据 | 结果 | 首次差异或备注 |
| --- | --- | --- | --- |
| Unit | `cargo test -p rust-quant-cli tradingview_velocity_parity --lib` | `13 passed; 0 failed` | 输出中的 warning 均来自既有无关模块 |
| Compile | `cargo check -p rust-quant-cli --bin tradingview_velocity_parity` | 通过 | 新增模块无新增 warning |
| Pine identity | `bundled_pine_source_matches_frozen_hash` | 通过 | `66d3937e` |
| Strict timing | next-open、锚点窗口、禁止回退、历史不足测试 | 通过 | `q∈[t-32,t-5]` 两端均包含 |
| RSI seed | `rsi_seed_ignores_the_missing_first_change` | 通过 | 首个14周期 RSI 位于索引14 |
| Broker path/DD | `intrabar_drawdown_uses_pre_trade_closed_equity_peak` | 通过 | 不把先有利后止损错误算成20点回撤 |
| BTC/ETH replay | Research CLI，结束时点 `2026-07-26 20:45 +08:00` | 通过 | 下表 |
| TradingView parity | 当前桌面 Strategy Tester + 逐笔审计 | BTC/ETH 均为历史 scenario comparison | Market 原始 K 线与图表历史边界未冻结，不能作为可重放 exact parity |
| Format/diff | scoped rustfmt 与 `git diff --check` | 通过 | 不格式化或清理用户无关改动 |
| File size | `wc -l` 迁移范围 Rust 文件 | 全部 `<2000` | 项目指定脚本不存在；`signals.rs` 超过1000目标但低于2000硬上限 |

### 5.1 BTC 多窗口

| 窗口 | 零成本：笔数 / 净利润 / PF / 平均R / 最大回撤 | 成本后：笔数 / 净利润 / PF / 平均R / 最大回撤 |
| ---: | --- | --- |
| 30天 | `7 / +265.00 / 1.178 / +0.295R / 976.20` | `7 / -434.47 / 0.770 / -0.152R / 1240.29` |
| 60天 | `9 / -1170.00 / 0.600 / +0.007R / 2411.20` | `9 / -2086.09 / 0.410 / -0.384R / 2891.92` |
| 90天 | `16 / +3344.00 / 1.855 / +0.549R / 2411.20` | `16 / +1553.84 / 1.317 / +0.171R / 2891.92` |

### 5.2 ETH 多窗口

| 窗口 | 零成本：笔数 / 净利润 / PF / 平均R / 最大回撤 | 成本后：笔数 / 净利润 / PF / 平均R / 最大回撤 |
| ---: | --- | --- |
| 30天 | `8 / +62.94 / 2.737 / +0.824R / 22.06` | `8 / +40.43 / 1.857 / +0.529R / 24.58` |
| 60天 | `15 / +304.56 / 5.360 / +2.347R / 35.86` | `15 / +262.30 / 4.020 / +1.980R / 41.93` |
| 90天 | `22 / +491.67 / 6.047 / +2.286R / 35.86` | `22 / +423.92 / 4.488 / +1.951R / 41.93` |

成本压力是固定1单位名义价格扣费，不是账户1%风险归一化组合回测。

## 6. Parity 与首个差异层

### 6.1 BTC

当时观测到 BTC 30天7笔的信号时间、下一根开盘、方向、退出时间、退出价和退出原因逐笔一致；净利润、Gross Profit/Loss、PF 与最大回撤也一致。这证明了该次规则回放没有发现首差异，但原始 Market K 线和图表可见历史并未以 DatasetManifest 固定，因此分类为 `scenario comparison`，不能称为未来迁移可重放的 exact parity。

### 6.2 ETH

- TradingView 当前图表共有26个订单，但读取接口只返回最后20个，即最后11笔完整交易；这些交易与 Rust 逐笔一致。
- TradingView 图表隐式加载边界包含一笔更早的 `1736.38→1729.60`，Rust 固定60天边界则额外包含 `05-28 -7.65` 与 `06-04 -25.96`。
- 将 Rust `304.56` 去掉这两笔，再加入图表边界交易 `-6.78`，得到 `331.39`；最后一笔止损 `1867.23` 与图表 `1867.26` 的 `0.03` tick/数据差调整后为 `331.42`。

首个可确认差异是历史加载边界，最后 `0.03` 是数据缓存修订或 tick 舍入口径；不是入场家族或未来数据差异。

### 6.3 本轮发现并修正的实现差异

1. **EMA 历史种子**：14天预热时 Rust 在 `2026-06-23` 多出 EMA 压缩扩张信号；60天预热后消失。
2. **RSI 首值**：首根 `ta.change(close)` 应为 `na`，不是0；修正后首值与 Pine 同索引。
3. **最大回撤**：仅按已平仓权益得到 BTC `860.10`；按 TradingView 公式后为 `976.20`。
4. **成本口径**：TradingView 当前为0佣金；真实成本必须作为独立压力层，不能混进 exact parity。

## 7. 架构与安全审计

- Research-only，版本为 `tradingview_velocity_parity_15m_research_v1`。
- 只读消费 Market 公共历史观察与 Strategy 规则工件；不读取用户凭证、不使用固定服务 API Key，也不拥有 Candle、universe 或 Strategy 发布事实。
- 不写数据库，不注册策略目录、Paper、Live、worker 或调度器。
- 不发送下单、撤单、平仓或任何交易所 mutation。
- 不改变 Core 现有生产事实源、执行门禁或旧策略身份。

## 8. Shadow、Cutover 与 Rollback

- Shadow：不需要；本工具本身就是离线 Research 对照。
- Cutover：不适用。
- Rollback：删除本迁移独立模块和 CLI 即可；旧策略不受影响。
- 生产状态：未切换。

## 9. Legacy Ratchet

旧 Rust V1～V13 保持不变；本次不删除旧实现、配置或回测证据，legacy delta 为0。

## 10. 阻塞、未知与后续

- 没有逐笔/tick或 Bar Magnifier 证据，同棒路径只能复刻 TradingView 默认 emulator，而不能证明真实市场成交顺序。
- TradingView 的隐式历史起点不能从汇总指标直接导出；ETH 前3笔受工具20条订单上限影响。
- 当前只有 BTC/ETH、固定结束时点和最多22笔样本；尚未完成其他币种、未见月份、参数邻域、资金费、组合仓位和收益集中度。
- `signals.rs` 为 `1340` 行，低于2000硬上限但高于1000目标；本次为避免非必要重构没有拆分，后续若继续新增家族，应先按 divergence/range/pattern 拆域。
- 要把这个历史记录升级为未来实施的依赖，先分别建立 Market `HistoricalDatasetV1` 和 Strategy `RuleArtifactV1` Owner 子 Manifest；Research 再建立只读 replay 子 Manifest，固定输入/输出 hash 并重新验证。

## 11. Verdict

- 技术 state：`verified`（`historical_characterization` 范围内的 Research 回放记录；不是当前 HEAD Verdict）。
- Promotion status：`research_only`。
- 技术结论：冻结 Pine 的核心信号、订单和风控已在独立 Rust Research 入口观测到一致；Market 输入未冻结，BTC/ETH 均只属于 scenario comparison。
- 收益结论：不能晋级。BTC 成本后30天、60天为负，90天 PF 仅 `1.317`；ETH 虽为正，但样本不足且跨币种不稳健。
- Cutover eligibility：不适用。
- Legacy delete eligibility：不适用。
- Cutover status：`not_required`。
- 是否含敏感数据：否。
