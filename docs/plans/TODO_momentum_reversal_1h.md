# TODO: 1H Momentum Reversal Strategy

## 📋 策略概要

**状态**: 探索阶段  
**创建日期**: 2026-07-09  
**目标**: 替代失败的 5m scalper，提供短周期（但数学可行）的策略

## 🎯 核心假设

```
当 BTC/ETH 在 1H 上出现以下条件时：
  1. RSI(14) < 30（超卖）或 > 70（超买）
  2. 价格偏离 EMA(20) > 2 ATR（动量衰竭）
  3. 出现反转 K 线形态（锤子线/吞噬）

则在未来 4-12H 内有反向运动 8-15% 的概率 > 58%
```

**与 Vegas 4H 的互补性**:
- Vegas 4H: 趋势跟随，持仓 1-7 天，月频次 5-10 笔
- New 1H: 短期反转，持仓 4-12 小时，月频次 20-40 笔 ✅

## 📊 探索模式结果（待填写）

**回测数据**: BTC 1H, 2026-04-01 ~ 2026-06-30 (3 个月)

| 指标 | 目标 | 实际 | 状态 |
|------|------|------|------|
| Win Rate | > 55% | ? | ⏳ |
| 月交易频次 | 20-40 笔 | ? | ⏳ |
| 月 PnL | > 10u | ? | ⏳ |
| Max DD | < 10% | ? | ⏳ |
| 平均 R 倍数 | > 1.2 | ? | ⏳ |

## ✅ 已完成

- [x] 创建探索日志 (`docs/exploration_log.md`)
- [x] 生成快速原型 (`tests/exploration_momentum_reversal_1h.rs`)
- [x] 定义核心假设
- [x] 创建 TODO 文档（本文件）

## 🔄 下一步（探索阶段）

### Step 1: 准备回测数据（优先）

**数据要求**:
- 币种: BTC-USDT-SWAP
- 周期: 1H
- 时间范围: 2026-04-01 ~ 2026-06-30（3 个月，约 2160 根 K 线）
- 包含: open, high, low, close, volume, timestamp

**数据导出方式**:

```bash
cd /Users/mac2/onions/crypto_quant/rust_quant

# 方式 1: 从数据库导出（推荐）
# TODO: 使用项目的数据导出工具

# 方式 2: 使用现有数据
# 检查是否有现成的 1H 数据
ls tests/fixtures/ | grep "1h"
```

**CSV 格式示例**:
```csv
timestamp,open,high,low,close,volume
1711929600000,68450.5,68920.3,68200.1,68750.2,1250000
...
```

### Step 2: 实现指标计算

当前原型使用占位符，需要实现或接入：

```rust
// 在 tests/exploration_momentum_reversal_1h.rs 中
// 替换占位符函数:

use rust_quant_indicators::momentum::RsiIndicator;
use rust_quant_indicators::trend::EmaIndicator;
use rust_quant_indicators::volatility::AtrIndicator;

fn calculate_simple_rsi(window: &[CandleItem], period: usize) -> f64 {
    let mut rsi = RsiIndicator::new(period);
    for candle in window {
        rsi.update(candle.close);
    }
    rsi.current_value()
}

// 类似地实现 EMA 和 ATR
```

### Step 3: 运行第一次回测

```bash
cd /Users/mac2/onions/crypto_quant/rust_quant

# 移除 #[ignore] 标记后运行
cargo test test_momentum_reversal_1h_exploration -- --nocapture
```

**预期输出**:
```
生成信号数: XX
总交易数: XX
胜率: XX%
月 PnL: XXu

🎯 决策建议: [✅ 有潜力 / ⚠️ 优化 / ❌ 不成立]
```

### Step 4: 参数调优（如果初次不达标）

调整方向:

| 参数 | 基准值 | 调整方向 | 影响 |
|------|--------|---------|------|
| RSI 阈值 | 30/70 | 25/75 或 35/65 | 信号数量 |
| 偏离度 | 2 ATR | 1.5 或 2.5 | 入场质量 |
| 止盈 R | 2.0 | 1.5 或 2.5 | 盈亏比 |
| 止损 ATR | 1.5x | 1.8x 或 2.0x | 胜率 vs 风险 |

### Step 5: 决策点

**如果 Win Rate > 55% 且月 PnL > 10u**:
- ✅ 升级到生产模式
- 创建设计文档 (`docs/plans/2026-07-XX-momentum-reversal-1h.md`)
- 进入完整回测阶段

**如果 Win Rate 52-55%**:
- ⚠️ 继续优化参数
- 尝试增加 MACD 背离确认
- 测试不同市场环境

**如果 Win Rate < 52%**:
- ❌ 记录失败到 `docs/exploration_log.md`
- 分析失败原因
- 考虑调整核心逻辑或尝试其他想法

## 🚀 生产模式计划（探索成功后）

### Phase 1: 完整回测（1 周）

- [ ] 扩展数据到 6 个月（2026-01 ~ 2026-06）
- [ ] 测试 ETH（Tier A 泛化性）
- [ ] 参数稳健性测试（±20% 扰动）
- [ ] 多市场环境验证（牛市/熊市/震荡）

### Phase 2: 分层测试（可选）

- [ ] 测试 Tier B 币种（AVAX/MATIC）
- [ ] 调整参数：ATR × 1.8, 入场质量 > 0.75
- [ ] 验证是否能跨 Tier 通用

### Phase 3: 集成到 strategies/

```
crates/strategies/src/implementations/momentum_reversal_1h/
├── mod.rs
├── types.rs          # MomentumReversalConfig
├── strategy.rs       # 核心逻辑
└── executor.rs       # StrategyExecutor trait
```

### Phase 4: Gate 5-8

- [ ] Gate 5: 风控契约（止损强制、仓位计算）
- [ ] Gate 6: 版本落位（v1.0.0）
- [ ] Gate 7: Shadow Trading → Paper Observation → ReadOnly
- [ ] Gate 8: 灰度上线（10% → 50% → 100%）

## 📝 关键决策记录

### 为什么选择 1H 而非 5m？

**5m 的问题**:
- ❌ Momentum Breakout: Win 69.5% 但 PnL -4.15u
- ❌ Range Reversion: Win 64.2% 但 PnL +3u（目标 20u）
- ❌ 费率占比 20-33%，数学天花板

**1H 的优势**:
- ✅ 费率占比 <5%（可接受）
- ✅ 信号质量更高（噪音少）
- ✅ 仍比 4H 快 4 倍（与 Vegas 互补）
- ✅ 月化目标现实化（10-15u 而非 20u）

### 为什么是"反转"而非"突破"？

原 Momentum Breakout 失败原因：
- ❌ Pullback 入场在熊市中捕获死猫跳
- ❌ 止盈/止损比例失衡（0.8 vs 2.0 ATR）

新 Reversal 策略优势：
- ✅ RSI 极值 + 动量衰竭更可靠
- ✅ 止盈/止损比例合理（2.0R）
- ✅ 反转形态增加确认（降低假信号）

## 📚 参考文档

- **方法论**: `docs/STRATEGY_ITERATION_METHODOLOGY.md`
- **探索日志**: `docs/exploration_log.md`
- **失败案例**: `implementations/momentum_breakout_scalper/DEPRECATED.md`
- **成功案例**: Vegas 4H（已上线）

## 🔗 相关链接

- 原型代码: `tests/exploration_momentum_reversal_1h.rs`
- 数据准备: （待补充）
- 回测结果: （待补充）

---

**最后更新**: 2026-07-09  
**维护者**: 策略研发团队  
**状态**: ⏳ 探索阶段 - 等待数据准备与首次回测
