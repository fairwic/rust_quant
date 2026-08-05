# Migration Evidence：MIG-20260727-tradingview-velocity-rust-parity-v2

## 1. 结论

本记录描述当时 Pine `3cbbc9d8` 被实现为独立 Rust Research 规则版本
`tradingview_velocity_parity_15m_research_v2`。旧
`tradingview_velocity_parity_15m_research_v1@66d3937e` 仍是默认规则，
没有被覆盖。

当时观察到 BTC 30天逐笔结果与 TradingView 一致；ETH 在归一化 TradingView
隐式图表历史边界后，信号、下一根开盘成交、保护位与退出未发现规则层首差异。
但 Market 原始 K 线、图表可见历史和 point-in-time universe 没有作为不可变
DatasetManifest 完整冻结，因此这些结果都只能归类为 Research `scenario comparison`，
不能作为未来目标目录迁移的 exact parity 或当前 HEAD Verdict。

严格 Top60 的历史记录使用当次 OKX `current-live` 合约，退市币在选择时排除。
这本身带幸存者偏差，只能作为 scenario。记录中出现的本地 K 线补数由既有
Market 数据准备链路执行；它不是 Research V2 的合法写入，也不能作为未来 Research
子 Manifest 的权限或依赖。未来若需 backfill，必须先完成 Market owner 的
`HistoricalDatasetV1`/DatasetManifest 子 Manifest，再把只读数据交给 Research。

完整60品种结果并未证明策略可晋级：零成本 PF 为 `1.1431`，每边5bps手续费
加3bps滑点后 PF 降为 `0.8347`、平均每笔 `-0.16292R`，仅 `12/60` 个品种
盈利。因此已记录的 Research 回放技术状态为 `verified`，`promotion_status` 为
`research_only`，`cutover_status` 为 `not_required`；策略研究结论为“完整60品种成本门禁失败”，
不是技术状态的一部分。

## 2. 身份与范围

| 字段 | 值 |
| --- | --- |
| Migration program / Owner child | `MP-tradingview-velocity-research-characterization / MIG-20260727-tradingview-velocity-rust-parity-v2 / Research` |
| Manifest kind / Registry | `historical_record / docs/architecture/migrations/programs/registry.toml` |
| 可作为 future dependency | `false` |
| Manifest SHA-256 | `f363dc343b3d163bfeb594e65f9081088f521296bbfbc70c3bd39ef3ed6d1599`；历史原件未留存 hash |
| 当前 Pine 编辑器口径 FNV-1a 32 | `3cbbc9d8` |
| 当前 Pine 原始文件 SHA-256 | `60d53b97f35bfe15bc885a21e23834f7be88b4afe4feaeebb205da20758cc910` |
| Rust V2 | `tradingview_velocity_parity_15m_research_v2@3cbbc9d8` |
| Rust V1 | `tradingview_velocity_parity_15m_research_v1@66d3937e` |
| Top60 manifest | `okx_surviving_static_top60_15m_20260727_v1` |
| 历史 Top60 selection fingerprint | `b3aa75157a7d17b3366e68060cdec3515b5b13355c22088ff2a7844ca44f96cf` |
| 当前保存的 selection 文件 SHA-256 | `8b68ad13a44841d12be67aeb270e6cd06fb8a6f4824db2e610454053207a50dd` |
| 行情成交量字段 | OKX `vol_ccy`；不计算 `volume × close` |
| 被测试代码 commit | `660407f438db52acde8318e705b26ad05948aa0a` |
| 范围补丁 SHA-256 | 历史未留存；不能生成当前 HEAD Verdict |
| Evidence scope / 技术状态 | `historical_characterization / verified` |
| Promotion / Cutover | `research_only / not_required` |

### 2.1 输入、输出与 Owner 边界

| 类型 | ref | SHA-256 | Owner/用途 |
| --- | --- | --- | --- |
| Strategy 行为输入 | `15min_velocity_all_symbol_strategy_research.pine` | `60d53b97f35bfe15bc885a21e23834f7be88b4afe4feaeebb205da20758cc910` | Strategy 规则工件，Research 只读 |
| Market universe 输入 | `tradingview_velocity_surviving_static_top60_selection_20260727.json` | `8b68ad13a44841d12be67aeb270e6cd06fb8a6f4824db2e610454053207a50dd` | 仅 universe 选择，不能替代 Candle DatasetManifest |
| 历史输出 | `tradingview_velocity_strict_top60...json` | `bd33b7ff011d608ff3d779d5a783c6118dfa6f87ab6e774b734a77fc9a3302fd` | 已记录的研究报告 |

Research V2 只消费 Market 历史数据和 Strategy 行为工件：不写 Candle、universe、Strategy release、`ExecutionRequest`、Account 或 Risk 事实；也不使用固定服务 API Key。`evidence.md` 不是动态输入。

实际实现没有新建第二套平行引擎，而是在同一 parity 模块中增加显式
`ParityRuleVersion::Current3cbbc9d8`。CLI 无参数仍选择
`Frozen66d3937e`；只有传入 `--rule-version current-v2` 才启用当前规则。
这样保留了 V1 的回放入口、身份与历史结果，同时避免复制 broker 和指标主链路。

本历史记录曾涉及：

- `crates/rust-quant-cli/src/app/tradingview_velocity_parity.rs`
- `crates/rust-quant-cli/src/app/tradingview_velocity_parity/`
- `crates/rust-quant-cli/src/bin/tradingview_velocity_parity.rs`
- `crates/rust-quant-cli/src/bin/tradingview_velocity_top60.rs`
- `crates/rust-quant-cli/src/bin/tradingview_velocity_strict_top60.rs`

其中历史 backfill/freeze 文件属于 Market 数据准备，不属于经修订后的 Research V2 scope；
后续 Research 子 Manifest 必须禁止修改它们。没有修改 Execution、Orchestration、Paper、Live、
生产 compose、CI/CD 或交易所 mutation 路径。

## 3. Pine V2 规则矩阵

| 新增家族 | Rust 规则与时序 | 独立退出 |
| --- | --- | --- |
| ENR 高位放量努力无结果空头 | 在 `t-20～t-80` 选择最近的最高高点锚点；失败不回退；次高、量能、RSI、强阴线和布林上轨收回均在 `t` 收盘确认 | 两高较高者上方1 tick；1R激活近似保本；1.5R全平 |
| BLR 布林下轨收回多头 | 完整量能、下轨收回、下影/收盘位置、前48根底部、RSI连续回升、负 MACD 柱连续收缩和 `>=1.1R` 均只读 `t` 及之前 | 信号低点下方1 tick；冻结信号时布林中轨 |
| EMA596-D2 收复接受后离轨多头 | 收复至少发生在1根前并持续站线；前棒回贴、当前离轨、HH/HL、周P90、20根中位量比及两条慢线联合负斜率否决均使用已完成 K 线 | 前4根低点下方1 tick；冻结2R目标 |

共享合同：

- 候选在信号 K 线收盘确认，最早下一根开盘成交。
- 同棒多空冲突不交易。
- 多头退出优先级为 `BLR > EMA596-D2 > 三棒反包 > 旧分支`。
- 空头退出优先级为 `ENR > 旧空头/过渡扫高分支`。
- 三个新家族只在 `Current3cbbc9d8` 启用，V1 单元测试继续走旧规则。

## 4. BTC / ETH 对账

评价结束时间固定为 `2026-07-26T20:45:00+08:00`，固定1单位。零成本层
用于 TradingView 对账；成本层是每边5 bps手续费加3 bps滑点等价的事后压力，
不会移动成交价、保护位或目标。

### 4.1 Rust V2 多窗口

| 标的 | 窗口 | 交易数 | 零成本净价差 | 零成本 PF | 成本压力净价差 | 成本压力 PF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| BTC-USDT | 30天 | 9 | `+1177.10` | `1.7908` | `+274.5674` | `1.146` |
| BTC-USDT | 60天 | 12 | `+1714.70` | `1.587` | `+492.6242` | `1.139` |
| BTC-USDT | 90天 | 19 | `+6228.70` | `2.592` | `+4132.5532` | `1.843` |
| ETH-USDT | 30天 | 9 | `+77.36` | `3.135` | `+52.2556` | `2.108` |
| ETH-USDT | 60天 | 16 | `+318.98` | `5.567` | `+274.1245` | `4.156` |
| ETH-USDT | 90天 | 23 | `+506.09` | `6.195` | `+435.7514` | `4.586` |

### 4.2 V1 保持不变

同一30天窗口：

- BTC V1：7笔，`+265.00`，PF `1.1780`；成本压力
  `-434.4698`，PF `0.770`。
- ETH V1：8笔，`+62.94`，PF `2.737`；成本压力
  `+40.4282`，PF `1.857`。

V2 对 BTC 增加：

- `2026-07-02 08:00Z` EMA596-D2 多单，`+428.10`。
- `2026-07-22 00:45Z` ENR 空单，`+484.00`。

`265.00 + 428.10 + 484.00 = 1177.10`，与 TradingView 30天统计完全一致。

### 4.3 TradingView 图表边界

BTC 当前图表为9笔、净利润 `+1177.10`、PF `1.790796`、最大回撤
`976.20`，与 Rust 30天逐笔一致。

ETH 当前图表为15笔已平仓、净利润 `+345.81`、PF `9.038354`、最大回撤
`40.84`，另有一笔未平仓空单。Rust 60天为16笔、`+318.98`。差异全部来自
图表隐式起始边界：

```text
318.98
+ 7.65     # 移除 Rust 边界内 2026-05-28 的亏损
+ 25.96    # 移除 Rust 边界内 2026-06-04 的亏损
- 6.78     # 加回 TradingView 边界处的较早亏损
= 345.81
```

归一化边界后，可见最后11笔及其信号、方向、入场、保护位、退出和 PnL
一致；没有发现规则层、时序层或成交层首差异。60天预热仍只是对
TradingView 隐式图表历史的近似，不等于已经冻结不可变行情 fixture。

## 5. Top60 严格回放

历史严格 runner 固定并重新审计：

- 当次 OKX `current-live` 成员、版本、原始 tick 与 manifest SHA-256；
- `2025-05-02 00:00Z` 起的60天预热；
- `2025-07-01 00:00Z ～ 2026-07-19 14:30Z` 正式评价窗口；
- `confirm='1'` 的15分钟 OHLC 与原生 `vol_ccy`；
- 每币首尾、内部15分钟连续性和内容指纹；
- 当时本地输入库为 `quant_core`；本项不是对 Research 直接读写的未来权限。

初次审计为 `0/60`。随后历史 Market 数据准备复用既有两段链路：

1. `tradingview_velocity_backfill_top60` 读取冻结 selection plan，以官方历史月包
   补齐到 `2026-06-30 15:45Z`；
2. `market_velocity_candle_backfill` 用既有 REST 增量链路补齐7月尾段。

增量阶段60个品种全部成功，写入或更新 `116334` 行，检测并修复17个品种的
尾段缺口。这些写入必须被记录为 Market owner 的历史数据准备，而不是 Research V2
的副作用或可复用的迁移证据。最终历史 sealed 记录：

| 字段 | 结果 |
| --- | ---: |
| 完整成员 | `60/60` |
| 每币预热 K 线 | `5760` |
| 每币评价 K 线 | `36826` |
| 总覆盖 K 线 | `2555160` |
| 行情字段 | `vol_ccy` |
| manifest SHA-256 | `b3aa75157a7d…44f96cf` |

正式报告：

`docs/backtest_reports/tradingview_velocity_strict_top60_okx_surviving_static_top60_15m_20260727_v1_b3aa75157a7d.json`

| 指标 | 零成本 | 每边5bps手续费 + 3bps滑点 |
| --- | ---: | ---: |
| 交易数 | `7687` | `7687` |
| Profit Factor | `1.1431` | `0.8347` |
| 胜率 | `31.65%` | `29.14%` |
| 平均净 R | `-0.00044R` | `-0.16292R` |
| 净 R | `-3.42R` | `-1252.34R` |
| 盈利品种 | `26/60` | `12/60` |
| 60分钟同向事件簇 | `3092` | `3092` |

`assert_cost_path_parity` 已通过：成本压力只改变净 PnL、净 R 和派生指标，
不改变信号、成交、止损、退出、阻塞信号或结束状态。移除前5笔盈利交易后，
成本压力净 R 进一步为 `-1300.65R`；移除前5个盈利品种后为 `-1323.11R`，
结论不依赖少数头部盈利。

固定1单位 raw PnL 仍混合了不同币价单位，不是统一资金、容量、相关性、杠杆
和并发约束下的组合权益曲线。该 cohort 还明确带 current-live 幸存者偏差；
它只能作为冻结的全量 Research scenario，不能冒充历史 point-in-time universe。

## 6. 历史验证结果

| 验证 | 结果 |
| --- | --- |
| `cargo test -p rust-quant-cli tradingview_velocity_parity --lib` | 29通过，0失败 |
| `cargo test -p rust-quant-cli --bin tradingview_velocity_top60` | 2通过，0失败 |
| `cargo test -p rust-quant-cli strict_static --lib` | 24通过，0失败 |
| `cargo test -p rust-quant-cli --bin tradingview_velocity_freeze_top60` | 2通过，0失败 |
| `cargo test -p rust-quant-cli --bin tradingview_velocity_backfill_top60` | 2通过，0失败 |
| `cargo test -p rust-quant-cli --bin tradingview_velocity_strict_top60` | 4通过，0失败 |
| scoped Research binary `cargo check` | 通过 |
| scoped `rustfmt` | 通过 |
| Pine V1/V2 identity tests | 通过 |
| BLR/ENR 退出优先级集成测试 | 通过 |
| Top60 默认完整覆盖 fail-close | 通过 |
| 严格 Top60 重新审计与正式回放 | `sealed=true`、`60/60` |
| 双成本路径状态对账 | 信号、成交、保护、退出、阻塞和结束状态一致 |
| 项目行数脚本 | 不存在，使用 `wc -l` 兜底 |
| 最大新 Rust 文件 | `signals.rs` 1544行，低于2000硬上限但超过1000目标 |

编译输出只有仓库已有的 dead-code、ambiguous glob re-export 和 unused import
警告；本切片未修改对应旧代码。

## 7. 历史副作用、未来边界与状态

- 历史数据库记录：按当时授权，既有 **Market** backfill 向本地 `quant_core` 幂等写入或更新
  `116334` 根15分钟 K 线；该行为超出 Research V2 scope，不得在未来 Research Manifest 中复用。
- 未来 Research V2：只读消费 Market owner 发布的 DatasetManifest/candle stream；`database_writes = false`，
  不触发 backfill。
- OKX：只读 instruments、历史月包与公共行情 REST，没有交易 mutation。
- TradingView：只恢复查看符号到 `OKX:ETHUSDT`，没有告警或交易 mutation。
- Paper/Live：未注册、未启动、未切换。
- 生产、CI/CD、部署：未触碰。
- Git：未提交、未推送。

分离后的状态：

```text
state = verified
promotion_status = research_only
cutover_status = not_required
research_outcome = not_promoted_full60_cost_gate_failed
```

要重新执行或继续优化，先建立 Market `HistoricalDatasetV1` Owner 子 Manifest（含
point-in-time universe、Candle 内容 hash 与数据来源证据）和 Strategy `RuleArtifactV2`
Owner 子 Manifest；Research 再以只读输入建立 `current_migration` replay。之后才可新增独立
策略版本、预先冻结未见窗口、统一资金与风险模型、候选容量及相关事件聚类规则；不得围绕
本轮已见亏损交易直接放宽阈值，也不得把 current-live cohort 解释成历史 point-in-time universe。
