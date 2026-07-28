# 重复造轮子 / 未用外部标准版 基线台账

- 冻结日期:2026-07-27
- 上位:[baseline README](README.md)、[owner-ledger](owner-ledger.md)、[dependency-rules §13](../../dependency-rules.md)
- 目的:登记迁移前项目中"重复实现"与"已引入依赖却未用到位/该用外部标准版却手写"的实证清单,作为新架构收敛目标与 xtask 防腐检查候选。**只登记,不在本文改代码。**

所有条目均带 `文件:行` 实证(2026-07-27 HEAD 扫描)。规模数字见文末速览。

## A. 未用外部标准版(已引入依赖却手写 / 该用成熟 crate)

| 键 | 问题 | 实证(节选) | 规模 | 应用方案 |
|---|---|---|---|---|
| W1 | **手写重试/退避循环**,`tokio-retry` 已在 orchestration 依赖树却零引用 | `market_funding_squeeze_reversal_research.rs:334`、`okx_historical_15m_backfill.rs:997`、`okx_historical_universe.rs:711/767`、`market_velocity_backfill.rs:756`、`market_cross_exchange_basis_panel/binance_klines.rs:271/688` 等 | 9+ 份,多份逐字重复(`0..4u64` + `250*(attempt+1)`) | 统一 `tokio-retry` 的 `Retry::spawn(ExponentialBackoff...)`,或引入 `backoff`(带 jitter);合并成一个共享 helper |
| W2 | **金额/价格用 f64**,`rust_decimal` 已声明(sqlx 已配 feature)却未贯穿领域层 | `domain/src/entities/position.rs:56/58`(pnl)、`domain/src/entities/audit.rs:57/85/87/89`、`domain/src/traits/strategy_trait.rs:30/37`、`execution/src/order_manager/swap_order_service.rs:10/568`、`domain/src/value_objects/stop_loss.rs:19/46` | 金额语义字段 f64:Decimal ≈ 275:27 | 成交价/数量/盈亏/余额用 `rust_decimal::Decimal`;指标/统计保留 f64 |
| W3 | **env 读取散落**,已有集中层 `core/src/config/env.rs`(`env_is_true`/`env_or_default`)却大面积绕过 | `std::env::var` 全仓 468 处 / 100+ 文件(bootstrap*/internal_server*/market_velocity_*/exchange_symbol_sync 等生产路径) | 468 处 | 收敛到 `config/env.rs` helper,或引入 `figment`/`config` 做分层加载 + 强类型 `Deserialize` 配置结构体 |
| W4 | **手写均值/方差/中位数/滑窗**,`ndarray` 已声明却基本手写,`statrs` 未引入 | `okx_historical_universe.rs:608 median`、`market_beta_residual_momentum_research.rs:912 mean`/`:672 variance`、`bb_rsi_strategy/executor.rs:103`、`range_reversion_scalper/strategy.rs:184` | 内联 `(x-mean).powi(2)` 方差非测试处 24 处 | 引入 `statrs` 或用 `ndarray`;至少 `mean/median/stddev` 收敛到 common 一份 |
| W5 | **reqwest 客户端封装重复**,各处 `Client::new()`/`builder()` 自定 timeout/UA | `exchange_symbol_sync.rs:331`、`binance_futures_http.rs:37`、`notification/telegram.rs:29`、`execution_task_client.rs:128` | 4+ 处 | 共享 http client factory(统一 timeout/UA/retry middleware,如 `reqwest-middleware`+`reqwest-retry`) |
| W6 | **手算时间/时区毫秒常量**,而非 `chrono::Duration`/`FixedOffset` | `market_funding_squeeze_reversal_research.rs:17`(`DAY_MS`/`OKX_ARCHIVE_UTC_OFFSET_MS`)、`market_beta_residual_momentum_research.rs:13`(`MS_8H` 重复定义)、`support_resistance.rs:84`(用了 deprecated `from_utc`/`from_timestamp_millis`) | 多文件重复常量 | `chrono::Duration::hours/FixedOffset::east_opt`;弃用 deprecated API |

**未造轮子(实证澄清,避免误报)**:base64/hex/hmac/sha 均正确用 crate(`binance_futures_http.rs:3`);JSON 统一 serde;CSV 仅读 fixture + 解析逗号配置串(非表格 CSV);`format!` 拼 SQL 仅 2 处且只插表名、值仍走 `$1` 绑定,无注入。

## B. 项目内部重复实现(同一件事多份)

| 键 | 问题 | 实证(节选) | 规模 | 收敛方案 |
|---|---|---|---|---|
| D1 | **回测/replay 主循环 5+ 套** | `strategies/framework/backtest/engine.rs:10 run_back_test`、`orchestration/backtest/{runner.rs:64,executor.rs:160}`、`services/strategy/{backtest_service.rs:27,live_parity.rs:168}`、MVE `equity.rs`+`equity/replay_*.rs`、`tradingview_velocity_parity/engine.rs:9` + 多个 bin | 5+ 主循环 | 单一 `backtest-engine` crate:`trait Strategy{on_candle}` + 通用 driver;live/paper/backtest 只换数据源与执行后端 |
| D2 | **指标手写多份**,`indicators` crate 已有权威版却被绕过 | ATR `atr_at` 逐字 6 份(`market_beta_residual_momentum_research.rs:882` 等)、EMA `ema_at` 4 份、RSI `compute_rsi` 多份、Bollinger `computed_candles.rs:193`、MACD `macd_v2.rs` | ATR 6 / EMA 4 / RSI 多 | 指标只能来自 `indicators` crate,删除全部 `*_at`/`compute_*` 局部实现;契约测试禁止在 indicators 外定义指标函数 |
| D3 | **止损/仓位/权益逻辑重复**(live 与 backtest 平行) | live `market_velocity_signal.rs:1490/1502/1460` vs backtest MVE `stop_loss.rs:123/136/210`;`calculate_best_stop_loss_price` 两份(`vegas/utils.rs:49`、`indicator_helpers.rs:593`) | 两条平行链 | `risk` crate 内 `StopLossPolicy`,live/backtest 共用;权益核算统一到引擎 `Ledger` |
| D4 | **K线模型/聚合/对齐重复**,8+ 个 OHLCV 结构体 | legacy `common CandleItem`、`domain Candle`、`market CandlesEntity`、MVE `BacktestCandle`、tv-parity `Candle`、`KlineScanCandle` 等;对齐两份(`confirmed_candle_aggregator.rs:463` vs `market_velocity_backfill.rs:538`) | 8+ 结构体 | canonical 内存模型归 `domains/market/model`；legacy `common::CandleItem` 是迁移来源，DB 实体/交易所 DTO/Testkit fixture 在边界映射；对齐/聚合统一到 Market 的一份实现 |
| D5 | **精度舍入/订单构建各写一份** | `round_price` 逐字 9 处(7 策略 `types.rs` + `market_velocity_signal.rs:1866` + bin);真实量化另一套 `execution_order_filters.rs:208/324/353`;tick_size 解析 4 处 | round_price 9 份 | 单一 `execution::precision::quantize(price/qty, tick/step)`,回测与 live 都传交易所 filters;禁止硬编码 `10_000.0` |
| D6 | **同名不同实现的 Service** | `CandleService` 两份(`market/repositories/candle_service.rs:12` vs `services/market/mod.rs:79`);`run_backtest_runner`/`run_back_test`/`run_back_test_strategy` 命名近似职责重叠 | — | 分层命名:`market::CandleService`=数据访问,services 层重命名或直接复用,禁同名 |
| D7 | **策略变体用整文件复制而非参数化** | `filtered_volume_rsi_ema_macd/` 下 16 个 `*_vN.rs`(v1..v13);`args.rs` 逐版 `_vN_research_args()`;单家族 `MARKET_MOMENTUM_OPPOSITE_MOVE` 37 个 MANIFEST | `*_research.rs` 24 / `*_vN.rs` 19 / `*MANIFEST*.md` 136 | 差异抽成声明式 `StrategyParams`(TOML/DB 行),v1..v13 收敛为 1 参数化策略 + N 配置;research 结果落数据表而非 markdown |
| D8 | **paper/backtest/live 信号评估三路重复** | backtest MVE `paper_signal.rs`/`directional_reversal.rs`/`equity/replay_strategy.rs`;live `market_velocity_signal.rs`(1984 行);paper `paper_signal.rs:32/53` | 近 4000 行镜像逻辑 | 单一纯函数 `SignalEvaluator`(candle 窗口+params→`SignalDecision`),三路共用;live-parity 升级为强制契约门禁 |
| D9 | **为 Dry-run 另造数据读取/回放引擎的风险** | 当前已有 D1/D4/D8 的多套 replay、Candle 与 signal 路径；若 B 阶段再次分别读取“当前”Market/Account/Instrument，会继续复制事实源与时序 | 未来风险，未宣称已实现 | 只允许一个 `B0 ImmutableDecisionEvidenceProvider` test-only Adapter；它消费各 owner 已冻结的 hash 工件并输出 `ImmutableDecisionEvidenceBundleV1`，不读取网络/数据库、不进入 runtime/Paper/Live/scheduler |

## C. 规模速览(实证)

| 指标 | 数值 |
|---|---|
| MVE backtest 目录 | 69 文件 / ~40,338 行 |
| strategies crate | 110 文件 / 21,554 行 |
| `atr_at` 逐字拷贝 | 6 份 |
| `round_price` 逐字拷贝 | 9 处 |
| `ema_at` 手写 | 4 份 |
| OHLCV/candle 结构体 | 8+ 个独立定义 |
| `std::env::var` 调用 | 468 处 / 100+ 文件 |
| 手写重试循环 | 9+ 份 |
| `*_research.rs` / `*_vN.rs` | 24 / 19 |
| `docs/*MANIFEST*.md` | 136(单家族最多 37 个 V*) |

## D. 优先级与 xtask 防腐候选

收益优先级:**W1 重试(依赖已就绪、9 份重复)> D2 指标绕过(6 份逐字 ATR)> D5 精度舍入(9 份 + 与真实量化脱节)> D7 变体复制(泛滥主因)> W2 f64 金额(正确性)> W3 env 散落 > D1/D3/D8 回测/信号收敛(工程量大,随搬块推进)**。

可静态落地为 xtask 检查(补进 [xtask-roadmap](xtask-roadmap.md)):

- **重试循环**:研究/CLI 层出现 `sleep(` + `attempt` 局部退避 → WARN,提示用 `tokio-retry`。误报中,先 WARN 不 ratchet。
- **指标绕过**:`indicators` 外定义 `fn atr_at/ema_at/compute_rsi` → 文件级 ratchet(基线冻结现有 6+4+N 份)。
- **round_price 硬编码**:非测试代码出现 `* 10_000.0).round()` 精度魔数 → 文件级 ratchet。
- **候选 candle 结构体**:`struct \w*Candle\w* {` 定义计数 → WARN(基线 8),禁新增。
- **B0 越界**:test-only evidence provider 出现在 App/runtime/Paper/Live/scheduler 依赖图，或包含网络/数据库写入 → FAIL；在 B0 外新增“当前 Market/Account/Instrument”fixture loader → WARN，要求复用唯一 bundle Contract。

需 AST/语义的(回测循环去重、信号评估合一、f64→Decimal 类型判定)不在 regex 范围,登记待 syn 阶段,不假装已覆盖。
