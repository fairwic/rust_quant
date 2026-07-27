# 阶段 0 基线:Workspace 依赖图与违规基线

- 状态:已冻结
- 冻结日期:2026-07-27
- `architecture_baseline_git_sha`:`660407f438db52acde8318e705b26ad05948aa0a`
- 上位:[迁移计划 阶段 0](../../migration-plan.md)、[依赖规则 §3/§13](../../dependency-rules.md)

本文件冻结迁移开始前的 workspace 内 crate 依赖现状与已知违规基线。它是 [`../baseline-2026-07/legacy-allowlist.toml`](legacy-allowlist.toml) 的推导依据,也是 `cargo xtask arch-check` ratchet 的语义参照。

## 1. Workspace 成员(冻结时 14 crate)

清理悬空 `rust-quant-ai-analysis` path 依赖后,根 `Cargo.toml` members 与 `crates/` 目录一致,共 14 个:

```
common, core, domain, infrastructure, services, trading,
market, indicators, strategies, risk, execution,
orchestration, analytics, rust-quant-cli
```

外部 path 依赖(不属迁移范围):`crypto_exc_all = { path = "../crypto_exc_all" }`;`[patch.crates-io] okx = { path = "../crypto_exc_all/okx_rs" }`。

## 2. 依赖方向(仅 workspace 内 path 依赖)

```text
common      -> (无)
domain      -> (无)
core        -> common
trading     -> domain
analytics   -> common, core, strategies          # ⚠ 违规:owner-agnostic 反向依赖业务层
market      -> common, core, domain
indicators  -> common, core, domain, market
strategies  -> common, core, domain, indicators, trading
risk        -> common, core, domain, market
execution   -> common, core, market, risk, strategies, indicators   # ⚠ 异常:依赖 strategies/indicators 却不依赖 domain
infrastructure -> common, core, domain
services    -> common, core, domain, infrastructure, market, indicators,
               strategies, trading, risk, execution, analytics
orchestration -> common, core, domain, infrastructure, services, market,
               strategies, indicators, risk, execution
cli         -> (全部 13 个)
```

## 3. 依赖分层(叶 -> 根,建议迁移顺序参考)

1. 零内部依赖:`common`、`domain`
2. 一层:`core`、`trading`
3. 二层:`market`、`infrastructure`
4. 三层:`indicators`、`risk`、`analytics`
5. 四层:`strategies`
6. 五层:`execution`
7. 聚合层:`services`
8. 编排层:`orchestration`
9. 入口:`rust-quant-cli`

## 4. 规模基线

| crate | 文件数 | 总行数 | >1000 行文件(非测试) |
|---|---|---|---|
| common | 13 | 653 | — |
| core | 13 | 1088 | — |
| domain | 38 | 4700 | — |
| infrastructure | 28 | 6528 | — |
| services | 102 | 44729 | market_velocity_signal 1984;execution_take_profit 1502;execution_audit 1435;execution_protection 1121 |
| trading | 5 | 216 | — |
| market | 23 | 4214 | — |
| indicators | 62 | 19999 | vegas/config 1367;liquidity_sweep_reversal 1273;trade_signal 1137 |
| strategies | 110 | 21554 | framework/backtest/position 1099;keltner_channel_scalper/strategy 1006 |
| risk | 24 | 2297 | — |
| execution | 5 | 1133 | — |
| orchestration | 48 | 8489 | infra/strategy_config 1146 |
| analytics | 17 | 3954 | — |
| rust-quant-cli | 246 | 107945 | 20+(最大 market_velocity_event_backtest ~1998) |

全仓约 734 个 .rs 文件 / 22.8 万行。cli 约 47%、services 约 20%。

## 5. 已知违规基线(冻结时)

| # | 违规 | 位置 | 目标规则 | 迁移归属 |
|---|---|---|---|---|
| V1 | owner-agnostic 层反向依赖业务层 | `analytics -> strategies` | dependency-rules §13-11 | analytics 迁 `quant/analytics` 时打断 |
| V2 | 依赖 strategies/indicators 却不依赖 domain,耦合方向偏重 | `execution` crate | dependency-rules §3/§4 | execution 迁 `domains/execution` 时纠正 |
| V3 | legacy signed read-only 账户直读 | `crates/risk/src/legacy_signed_read_only.rs` | dependency-rules §13-17 精神 | 收敛到 reconciliation + quant-web adapter |
| V4 | 同上 | `crates/execution/src/order_manager/order_service.rs`、`swap_order_service.rs` | 同上 | 同上 |
| V5 | 同上 | `crates/infrastructure/src/exchanges/okx_adapter.rs` | 同上 | 同上 |

跨库直连:**已治理,基线为 0**。quant_web 走 HTTP(`crates/services/src/rust_quan_web/execution_task_client.rs`);quant_core DB 单例有 URL 门禁(`crates/core/src/database/sqlx_pool.rs`,拒绝指向 quant_web);无 quant_news 直连。

## 6. 冻结口径

- 本图基于 `cargo metadata` 与逐 crate `Cargo.toml` path 依赖静态盘点,不含外部 crates.io 传递依赖。
- V1–V5 是 ratchet 允许存在的 legacy 基线,登记在 `legacy-allowlist.toml`;迁移期只允许违规数下降,禁止新增。
