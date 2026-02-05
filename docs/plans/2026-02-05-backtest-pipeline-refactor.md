# Backtest Pipeline Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 以 backtest_id=41 为基准，硬删除未启用模块与关闭的风控分支，回测仅保留 pipeline 路径，提升可读性。

**Architecture:** 删除 legacy backtest engine 与对比逻辑；pipeline 成为唯一回测入口。按模块域分批删除 MarketStructure/FakeBreakout、关闭的风控分支及其配置/日志/结构字段，确保全链路一致。

**Tech Stack:** Rust, rust-quant-strategies/backtest pipeline, rust-quant-indicators, orchestration/services.

---

### Task 1: 回测入口仅保留 pipeline

**Files:**
- Modify: `crates/strategies/src/framework/backtest/engine.rs`
- Modify: `crates/strategies/src/framework/backtest/adapter.rs`
- Modify: `crates/strategies/src/framework/backtest/mod.rs`
- Modify: `crates/strategies/src/framework/backtest/trait_impl.rs`
- Modify: `crates/strategies/src/framework/backtest/adapter.rs` (tests)

**Step 1: 写一个针对 pipeline-only API 的失败测试**

```rust
#[test]
fn pipeline_backtest_runs_and_records_trades() {
    use crate::framework::backtest::adapter::run_indicator_strategy_backtest;
    use crate::framework::backtest::types::BasicRiskStrategyConfig;

    #[derive(Debug, Clone, Default)]
    struct Strategy;
    impl crate::framework::backtest::adapter::IndicatorStrategyBacktest for Strategy {
        type IndicatorCombine = ();
        type IndicatorValues = ();
        fn min_data_length(&self) -> usize { 3 }
        fn init_indicator_combine(&self) -> Self::IndicatorCombine { () }
        fn build_indicator_values(_: &mut Self::IndicatorCombine, _: &crate::CandleItem) -> Self::IndicatorValues { () }
        fn generate_signal(&mut self, candles: &[crate::CandleItem], _: &mut Self::IndicatorValues, _: &BasicRiskStrategyConfig) -> crate::framework::backtest::types::SignalResult {
            let mut s = crate::framework::backtest::types::SignalResult::default();
            s.ts = candles.last().unwrap().ts;
            s.open_price = candles.last().unwrap().c;
            if s.ts % 2 == 0 { s.should_buy = true; }
            s
        }
    }

    let candles: Vec<crate::CandleItem> = (0..800)
        .map(|i| crate::CandleItem { o:100.0, h:101.0, l:99.0, c:100.0, v:1.0, ts:i, confirm:1 })
        .collect();
    let mut risk = BasicRiskStrategyConfig::default();
    risk.max_loss_percent = 1.0;

    let result = run_indicator_strategy_backtest("TEST", Strategy::default(), &candles, risk);
    assert!(result.open_trades > 0);
}
```

**Step 2: 运行测试，预期失败**

Run: `cargo test -p rust-quant-strategies pipeline_backtest_runs_and_records_trades -v`

Expected: FAIL（`run_indicator_strategy_backtest` 尚未迁移为 pipeline-only 接口或签名不匹配）。

**Step 3: 实现 pipeline-only 入口**

- 在 `engine.rs` 移除 `run_back_test`/`run_back_test_generic`，保留并重命名 pipeline 入口为 `run_back_test`。
- 在 `adapter.rs` 移除 legacy `run_indicator_strategy_backtest` + `run_indicator_strategy_backtest_pipeline` 双接口，保留 **单一** `run_indicator_strategy_backtest` 并调用 pipeline。
- 删除 pipeline 对比 legacy 的测试。
- 清理 `mod.rs` re-exports 与 `trait_impl.rs` imports。

**Step 4: 运行测试，预期通过**

Run: `cargo test -p rust-quant-strategies pipeline_backtest_runs_and_records_trades -v`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/strategies/src/framework/backtest/engine.rs \
       crates/strategies/src/framework/backtest/adapter.rs \
       crates/strategies/src/framework/backtest/mod.rs \
       crates/strategies/src/framework/backtest/trait_impl.rs

git commit -m "refactor(回测): 统一 pipeline 回测入口"
```

---

### Task 2: 移除 MarketStructure 模块（指标/配置/信号链路）

**Files:**
- Delete: `crates/indicators/src/pattern/market_structure_indicator.rs`
- Modify: `crates/indicators/src/pattern/mod.rs`
- Modify: `crates/indicators/src/trend/vegas/config.rs`
- Modify: `crates/indicators/src/trend/vegas/indicator_combine.rs`
- Modify: `crates/indicators/src/trend/vegas/signal.rs`
- Modify: `crates/indicators/src/trend/vegas/strategy.rs`
- Modify: `crates/strategies/src/framework/backtest/indicators.rs`
- Modify: `crates/orchestration/src/infra/strategy_config.rs`
- Modify: `crates/orchestration/src/infra/job_param_generator.rs`

**Step 1: 删除 MarketStructure 指标与配置结构**

- 删除 `market_structure_indicator` 模块与导出。
- 从 Vegas 指标配置中移除 `MarketStructureConfig` 及字段。
- 从 `indicator_combine` 与 `signal` 结构中移除 `market_structure` 相关字段。

**Step 2: 删除 Vegas 策略里的结构体信号逻辑**

- 移除 `market_structure_signal` 字段、默认值、初始化逻辑。
- 移除 `SignalCondition::MarketStructure` 的生成与权重参与。

**Step 3: 更新 backtest 指标构建与 orchestration 配置映射**

- `strategies/framework/backtest/indicators.rs` 删除 market_structure indicator 的计算。
- `orchestration/infra/strategy_config.rs` 与 `job_param_generator.rs` 删除相关字段映射。

**Step 4: 运行测试**

Run: `cargo test -p rust-quant-indicators -v`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/indicators/src/pattern/mod.rs \
       crates/indicators/src/trend/vegas/config.rs \
       crates/indicators/src/trend/vegas/indicator_combine.rs \
       crates/indicators/src/trend/vegas/signal.rs \
       crates/indicators/src/trend/vegas/strategy.rs \
       crates/strategies/src/framework/backtest/indicators.rs \
       crates/orchestration/src/infra/strategy_config.rs \
       crates/orchestration/src/infra/job_param_generator.rs

git commit -m "refactor(指标): 移除市场结构信号链路"
```

---

### Task 3: 移除 FakeBreakout 与 MarketStructure 权重定义

**Files:**
- Modify: `crates/indicators/src/trend/signal_weight.rs`

**Step 1: 写失败测试（确保权重列表不含 FakeBreakout/MarketStructure）**

```rust
#[test]
fn default_weights_exclude_removed_signals() {
    let cfg = SignalWeightsConfig::default();
    let has_removed = cfg.weights.iter().any(|(t, _)| matches!(t, SignalType::MarketStructure | SignalType::FakeBreakout));
    assert!(!has_removed);
}
```

**Step 2: 运行测试，预期失败**

Run: `cargo test -p rust-quant-indicators default_weights_exclude_removed_signals -v`

Expected: FAIL

**Step 3: 实现删除**

- 从 `SignalType` 移除 `MarketStructure` 与 `FakeBreakout`。
- 从 `SignalCondition` 移除对应变体。
- 删除相关评估逻辑与默认权重。
- 移除与 MarketStructure 相关的单测。

**Step 4: 运行测试，预期通过**

Run: `cargo test -p rust-quant-indicators default_weights_exclude_removed_signals -v`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/indicators/src/trend/signal_weight.rs

git commit -m "refactor(权重): 移除假突破与市场结构信号"
```

---

### Task 4: 移除逆势回调止盈（counter_trend）功能

**Files:**
- Delete: `crates/indicators/src/trend/counter_trend.rs`
- Modify: `crates/indicators/src/trend/mod.rs`
- Modify: `crates/strategies/src/framework/backtest/types.rs`
- Modify: `crates/strategies/src/framework/backtest/position.rs`
- Modify: `crates/strategies/src/framework/backtest/risk.rs`
- Modify: `crates/strategies/src/framework/backtest/conversions.rs`
- Modify: `crates/strategies/src/implementations/vegas_strategy.rs` (若存在)
- Modify: `crates/strategies/src/implementations/nwe_strategy.rs`
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`
- Modify: `crates/domain/src/value_objects/signal.rs`
- Modify: `crates/domain/src/entities/strategy_config.rs`
- Modify: `crates/orchestration/src/infra/job_param_generator.rs`
- Modify: `crates/orchestration/src/infra/strategy_config.rs`
- Modify: `crates/orchestration/src/infra/signal_logger.rs`

**Step 1: 写失败测试（SignalResult 不再包含 counter_trend）**

```rust
#[test]
fn signal_result_has_no_counter_trend_field() {
    let s = SignalResult::default();
    // 编译期保证：counter_trend 字段不存在
    let _ = s.should_buy; // keep test non-empty
}
```

**Step 2: 运行测试，预期失败**

Run: `cargo test -p rust-quant-strategies signal_result_has_no_counter_trend_field -v`

Expected: FAIL（字段尚未移除）

**Step 3: 实现移除**

- 删除 `counter_trend` 模块与导出。
- 从 `SignalResult` / `TradePosition` 删除 `counter_trend_pullback_take_profit_price`。
- 移除 `CounterTrendSignalResult` trait 依赖与实现。
- 删除 `risk.rs` 中 counter-trend TP 检查与相关调用。
- 删除 `position.rs` 中 counter-trend 赋值逻辑。
- 删除 `strategy_config` / `job_param_generator` 中 `is_counter_trend_pullback_take_profit`。
- 更新所有 struct literal 初始化处（services/orchestration/tests）。

**Step 4: 运行测试**

Run: `cargo test -p rust-quant-strategies -v`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/indicators/src/trend/mod.rs \
       crates/strategies/src/framework/backtest/types.rs \
       crates/strategies/src/framework/backtest/position.rs \
       crates/strategies/src/framework/backtest/risk.rs \
       crates/strategies/src/framework/backtest/conversions.rs \
       crates/strategies/src/implementations/nwe_strategy.rs \
       crates/services/src/strategy/strategy_execution_service.rs \
       crates/domain/src/value_objects/signal.rs \
       crates/domain/src/entities/strategy_config.rs \
       crates/orchestration/src/infra/job_param_generator.rs \
       crates/orchestration/src/infra/strategy_config.rs \
       crates/orchestration/src/infra/signal_logger.rs

git commit -m "refactor(风控): 移除逆势回调止盈链路"
```

---

### Task 5: 移除保本移动止损与单K振幅止损分支

**Files:**
- Modify: `crates/strategies/src/framework/backtest/types.rs`
- Modify: `crates/strategies/src/framework/backtest/position.rs`
- Modify: `crates/strategies/src/framework/backtest/risk.rs`
- Modify: `crates/strategies/src/framework/backtest/conversions.rs`
- Modify: `crates/orchestration/src/infra/job_param_generator.rs`
- Modify: `crates/orchestration/src/infra/strategy_config.rs`
- Modify: `crates/orchestration/src/infra/progress_manager.rs`
- Modify: `crates/services/src/trading/order_creation_service.rs`
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`
- Modify: `crates/domain/src/value_objects/signal.rs`

**Step 1: 写失败测试（BasicRiskStrategyConfig 不再含两项开关）**

```rust
#[test]
fn risk_config_has_no_move_or_one_k_flags() {
    let _cfg = BasicRiskStrategyConfig::default();
    // 编译期保证：字段被移除
}
```

**Step 2: 运行测试，预期失败**

Run: `cargo test -p rust-quant-strategies risk_config_has_no_move_or_one_k_flags -v`

Expected: FAIL

**Step 3: 实现移除**

- 从 `BasicRiskStrategyConfig` 删除：
  - `is_one_k_line_diff_stop_loss`
  - `is_move_stop_open_price_when_touch_price`
- 从 `SignalResult` / `TradePosition` 删除 `move_stop_open_price_when_touch_price` 字段。
- 删除 `risk.rs` 中 `check_one_k_line_diff_stop` 与 `activate_break_even_stop` 链路。
- 删除 `position.rs` 中相关字段赋值/处理。
- 更新 conversions 与所有 struct literal 初始化。

**Step 4: 运行测试**

Run: `cargo test -p rust-quant-strategies -v`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/strategies/src/framework/backtest/types.rs \
       crates/strategies/src/framework/backtest/position.rs \
       crates/strategies/src/framework/backtest/risk.rs \
       crates/strategies/src/framework/backtest/conversions.rs \
       crates/orchestration/src/infra/job_param_generator.rs \
       crates/orchestration/src/infra/strategy_config.rs \
       crates/orchestration/src/infra/progress_manager.rs \
       crates/services/src/trading/order_creation_service.rs \
       crates/services/src/strategy/strategy_execution_service.rs \
       crates/domain/src/value_objects/signal.rs

git commit -m "refactor(风控): 移除保本止损与单K止损"
```

---

### Task 6: 移除 validate_signal_tp 与 tighten_vegas_risk

**Files:**
- Modify: `crates/strategies/src/framework/backtest/types.rs`
- Modify: `crates/strategies/src/framework/backtest/position.rs`
- Modify: `crates/orchestration/src/backtest/executor.rs`
- Modify: `crates/orchestration/src/infra/job_param_generator.rs`

**Step 1: 写失败测试（risk_config 不包含 validate/tighten）**

```rust
#[test]
fn risk_config_has_no_validate_or_tighten_flags() {
    let _cfg = BasicRiskStrategyConfig::default();
}
```

**Step 2: 运行测试，预期失败**

Run: `cargo test -p rust-quant-strategies risk_config_has_no_validate_or_tighten_flags -v`

Expected: FAIL

**Step 3: 实现移除**

- 删除 `BasicRiskStrategyConfig.validate_signal_tp` 与 `tighten_vegas_risk`。
- 删除 `position.rs` 中 `validate_signal_tp` 校验逻辑。
- 删除 `backtest/executor.rs` 的 `tighten_vegas_risk` 函数与调用。
- 清理 job_param_generator 中相关字段。

**Step 4: 运行测试**

Run: `cargo test -p rust-quant-orchestration -v`

Expected: PASS

**Step 5: Commit**

```bash
git add crates/strategies/src/framework/backtest/types.rs \
       crates/strategies/src/framework/backtest/position.rs \
       crates/orchestration/src/backtest/executor.rs \
       crates/orchestration/src/infra/job_param_generator.rs

git commit -m "refactor(风控): 移除校验与vegas收紧开关"
```

---

### Task 7: 全链路清理与验证

**Files:**
- Modify: `crates/orchestration/src/infra/signal_logger.rs`
- Modify: `crates/strategies/src/framework/backtest/conversions.rs`
- Modify: `crates/services/tests/okx_simulated_order_flow.rs`
- Modify: `crates/services/src/trading/order_creation_service.rs`
- Modify: `crates/strategies/src/implementations/vegas_executor.rs`
- Modify: `crates/strategies/src/implementations/nwe_executor.rs`

**Step 1: 统一所有 SignalResult/BasicRiskStrategyConfig 初始化**

- 用 `..Default::default()` 或删除已移除字段
- 保证 `cargo test -q` 能通过

**Step 2: 全量测试**

Run: `cargo test -q`

Expected: PASS

**Step 3: Commit**

```bash
git add crates/orchestration/src/infra/signal_logger.rs \
       crates/strategies/src/framework/backtest/conversions.rs \
       crates/services/tests/okx_simulated_order_flow.rs \
       crates/services/src/trading/order_creation_service.rs \
       crates/strategies/src/implementations/vegas_executor.rs \
       crates/strategies/src/implementations/nwe_executor.rs

git commit -m "refactor(回测): 清理信号初始化与日志链路"
```

---

Plan complete and saved to `docs/plans/2026-02-05-backtest-pipeline-refactor.md`.

Two execution options:

1. Subagent-Driven (this session) – I dispatch a fresh subagent per task, review between tasks.
2. Parallel Session (separate) – Open new session with executing-plans, batch execution with checkpoints.

Which approach?
