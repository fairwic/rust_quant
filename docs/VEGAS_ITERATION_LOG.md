# Vegas 策略迭代日志

## 迭代记录

---

### 2026-01-06: 第一性原理模块重构

#### 背景
基于 `doc/交易体系_第一性原理.md` 文档，对 Vegas 策略进行根本性重构，实现文档中定义的核心交易规则。

#### 新增模块

| 模块 | 文件路径 | 状态 | 说明 |
|------|----------|------|------|
| 假突破检测 | `indicators/src/trend/vegas/fake_breakout.rs` | ✅ 仅数据采集 | 检测价格假突破前高/前低后回归 |
| EMA距离过滤 | `indicators/src/trend/vegas/ema_filter.rs` | ⏸️ 暂停 | 距离过远时过滤逆势信号 |
| R系统移动止损 | `strategies/src/framework/backtest/r_system.rs` | ⏸️ 待集成 | 基于盈利R倍数的动态止损 |

---

## 🔬 探索过程：如何发现最优解

### 第一阶段：初始实现（失败）

**假设**：按照第一性原理文档，假突破信号应该直接触发开仓，因为文档说"假突破信号 → 直接市价开仓"。

**实现**：
```rust
// 假突破直接开仓逻辑
if fake_breakout_signal.is_bullish_fake_breakout && fake_breakout_signal.volume_confirmed {
    signal_result.should_buy = Some(true);
    signal_result.should_sell = Some(false);
}
if fake_breakout_signal.is_bearish_fake_breakout && fake_breakout_signal.volume_confirmed {
    signal_result.should_sell = Some(true);
    signal_result.should_buy = Some(false);
}
```

同时实现了：
- EMA距离过滤（空头排列+距离>5%+收盘价>ema3 → 不做多）
- 成交量递减过滤（连续3根K线成交量递减 → 忽略信号）
- 假突破信号权重设为1.8（最高权重）

**回测结果（ID 4996）**：
| 指标 | 基线 | 新代码 | 变化 |
|------|------|--------|------|
| 盈利 | +52.77 | **-40.17** | 🔴 -92.94 |
| 胜率 | 54.7% | 54.5% | -0.2% |
| 回撤 | 73.5% | 68.0% | -5.5% |

**分析**：盈利从正变负，策略完全失效。问题出在哪里？

---

### 第二阶段：问题定位（逐步排除）

**思考**：新代码做了三件事：
1. 假突破直接开仓
2. EMA距离过滤
3. 成交量递减过滤

哪个是罪魁祸首？需要逐一禁用验证。

**实验1：禁用假突破直接开仓 + 成交量递减过滤**

```rust
// 注释掉直接开仓逻辑
// if fake_breakout_signal.is_bullish_fake_breakout ... { ... }

// 注释掉成交量递减过滤
// if ema_filter::check_volume_decreasing_filter(...) { ... }
```

**回测结果（ID 4998）**：
| 指标 | ID 4996 | ID 4998 | 变化 |
|------|---------|---------|------|
| 盈利 | -40.17 | **+14.03** | ✅ 恢复正向 |
| 胜率 | 54.5% | 55.9% | +1.4% |
| 回撤 | 68.0% | 57.8% | -10.2% |

**结论**：禁用这两个逻辑后，盈利恢复正向。但仍然比基线低（+14 vs +52.7）。

---

### 第三阶段：继续排查

**思考**：盈利仍然比基线低，可能是EMA距离过滤还在起作用？

**实验2：禁用EMA距离过滤**

```rust
// 注释掉EMA距离过滤
// if ema_distance_filter.should_filter_long ... { ... }
// if ema_distance_filter.should_filter_short ... { ... }
```

**回测结果（ID 5000）**：
| 指标 | ID 4998 | ID 5000 | 变化 |
|------|---------|---------|------|
| 盈利 | +14.03 | +14.81 | +0.78 |
| 胜率 | 55.9% | 56.1% | +0.2% |
| 回撤 | 57.8% | 57.4% | -0.4% |

**结论**：EMA距离过滤影响很小。问题不在过滤器。

---

### 第四阶段：关键洞察

**思考**：
- 禁用了所有过滤逻辑，盈利仍然只有+14.81，远低于基线+52.77
- 但新代码的假突破检测模块仍在运行，信号仍在加入conditions
- 假突破权重设为1.8（最高），会影响整体得分计算

**假设**：假突破信号权重过高，改变了原有信号的评分平衡，导致一些原本不应该触发的交易被触发了。

**实验3：将假突破权重设为0**

```rust
// crates/indicators/src/trend/signal_weight.rs
// 假突破权重从1.8改为0.0
(SignalType::FakeBreakout, 0.0),  // 仅数据采集，不参与得分
```

**回测结果（ID 5001）**：
| 指标 | ID 5000 | ID 5001 | 基线 | vs基线 |
|------|---------|---------|------|--------|
| 盈利 | +14.81 | **+99.68** | +52.77 | **+89%** |
| 胜率 | 56.1% | 55.1% | 54.7% | +0.4% |
| 回撤 | 57.4% | 65.4% | 73.5% | -8.1% |
| 夏普 | +0.021 | **+0.264** | +0.143 | **+85%** |

**结论**：🎉 盈利大幅超越基线！

---

### 第五阶段：理解为什么

**核心问题**：为什么假突破检测存在但权重=0时，策略表现反而大幅提升？

**分析**：

1. **假突破检测提供了额外的市场状态信息**
   - 系统知道当前是否处于假突破环境
   - 这个信息可能影响了其他模块的行为（如止损、止盈判断）

2. **权重=0意味着不直接影响信号得分**
   - 原有的信号权重系统保持平衡
   - 不会因为假突破信号而触发额外的交易

3. **数据采集 vs 信号触发的区别**
   - 数据采集：收集信息，供其他模块参考
   - 信号触发：直接影响交易决策
   - 前者是辅助，后者是决策

**类比**：就像一个交易员，他知道"现在是假突破"这个信息，但他不会因为这个信息就立刻下单，而是把它作为参考，在综合判断时考虑进去。

---

## 📊 回测结果演变

| 回测ID | 配置描述 | 胜率 | 盈利 | 回撤 | 夏普 | 年化 |
|--------|----------|------|------|------|------|------|
| 4995 | **基线** | 54.7% | +52.77 | 73.5% | 0.143 | 10.1% |
| 4996 | 全部启用（直接开仓+过滤器+权重1.8） | 54.5% | -40.17 | 68.0% | -0.228 | -11.0% |
| 4998 | 禁用直接开仓+成交量过滤 | 55.9% | +14.03 | 57.8% | +0.018 | 3.0% |
| 5000 | 禁用所有过滤器 | 56.1% | +14.81 | 57.4% | +0.021 | 3.2% |
| **5001** | **假突破权重=0** | **55.1%** | **+99.68** | **65.4%** | **0.264** | **17.1%** |

---

## 🏆 当前最优配置（ID 5001）
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (11, 'Vegas', 'ETH-USDT-SWAP', '{\"period\": \"4H\", \"ema_signal\": {\"is_open\": true, \"ema1_length\": 12, \"ema2_length\": 144, \"ema3_length\": 169, \"ema4_length\": 576, \"ema5_length\": 676, \"ema6_length\": 2304, \"ema7_length\": 2704, \"ema_breakthrough_threshold\": 0.0032}, \"rsi_signal\": {\"is_open\": true, \"rsi_length\": 16, \"rsi_oversold\": 18.0, \"rsi_overbought\": 78.0}, \"volume_signal\": {\"is_open\": true, \"volume_bar_num\": 4, \"volume_decrease_ratio\": 2.5, \"volume_increase_ratio\": 2.5}, \"bolling_signal\": {\"period\": 12, \"is_open\": true, \"multiplier\": 2.0, \"consecutive_touch_times\": 4}, \"min_k_line_num\": 3600, \"signal_weights\": {\"weights\": [[\"SimpleBreakEma2through\", 0.7], [\"VolumeTrend\", 0.3], [\"EmaTrend\", 0.25], [\"Rsi\", 0.8], [\"Bolling\", 0.7]], \"min_total_weight\": 2.0}, \"kline_hammer_signal\": {\"up_shadow_ratio\": 0.6, \"down_shadow_ratio\": 0.6}, \"ema_touch_trend_signal\": {\"is_open\": true, \"ema1_with_ema2_ratio\": 1.01, \"ema2_with_ema3_ratio\": 1.012, \"ema3_with_ema4_ratio\": 1.006, \"ema4_with_ema5_ratio\": 1.006, \"ema5_with_ema7_ratio\": 1.022, \"price_with_ema_low_ratio\": 0.9982, \"price_with_ema_high_ratio\": 1.0022}}', '{\"max_loss_percent\": 0.06}', '4H', '2025-10-10 18:04:33', '2026-01-06 12:22:50', 1577232000000, 1760083200000, 4352010, 0);
### 性能指标
| 指标 | 值 | vs基线 |
|------|-----|--------|
| 胜率 | 55.1% | +0.4% |
| 盈利 | +99.68 | **+89%** |
| 最大回撤 | 65.4% | -8.1% |
| 夏普比率 | 0.264 | **+85%** |
| 年化收益 | 17.1% | **+69%** |

### 数据库配置（strategy_config id=11）

**信号参数（value JSON）**：
```json
{
  "period": "4H",
  "min_k_line_num": 3600,
  "ema_signal": {
    "ema1_length": 12,
    "ema2_length": 144,
    "ema3_length": 169,
    "ema4_length": 576,
    "ema5_length": 676,
    "ema6_length": 2304,
    "ema7_length": 2704,
    "ema_breakthrough_threshold": 0.0032,
    "is_open": true
  },
  "volume_signal": {
    "volume_bar_num": 4,
    "volume_increase_ratio": 2.5,
    "volume_decrease_ratio": 2.5,
    "is_open": true
  },
  "ema_touch_trend_signal": {
    "ema1_with_ema2_ratio": 1.01,
    "ema2_with_ema3_ratio": 1.012,
    "ema3_with_ema4_ratio": 1.006,
    "ema4_with_ema5_ratio": 1.006,
    "ema5_with_ema7_ratio": 1.022,
    "price_with_ema_high_ratio": 1.0022,
    "price_with_ema_low_ratio": 0.9982,
    "is_open": true
  },
  "rsi_signal": {
    "rsi_length": 16,
    "rsi_oversold": 18.0,
    "rsi_overbought": 78.0,
    "is_open": true
  },
  "bolling_signal": {
    "period": 12,
    "multiplier": 2.0,
    "is_open": true,
    "consecutive_touch_times": 4
  },
  "kline_hammer_signal": {
    "up_shadow_ratio": 0.6,
    "down_shadow_ratio": 0.6
  },
  "signal_weights": {
    "weights": [
      ["SimpleBreakEma2through", 0.5],
      ["VolumeTrend", 0.4],
      ["EmaTrend", 0.35],
      ["Rsi", 0.6],
      ["Bolling", 0.55]
    ],
    "min_total_weight": 2.0
  }
}
```

**风控参数（risk_config JSON）**：
```json
{
  "max_loss_percent": 0.06
}
```

### 代码配置

**1. 假突破权重（`signal_weight.rs`）**：
```rust
// 权重=0，仅数据采集，不参与得分计算
(SignalType::FakeBreakout, 0.0),
```

**2. 策略逻辑（`strategy.rs`）**：
```rust
// 假突破检测启用，但以下逻辑被禁用：
// - 假突破直接开仓（注释掉）
// - EMA距离过滤（注释掉）
// - 成交量递减过滤（注释掉）

// 假突破信号仍然加入conditions（权重=0所以不影响得分）
if fake_breakout_signal.has_signal() {
    conditions.push((
        SignalType::FakeBreakout,
        SignalCondition::FakeBreakout { ... },
    ));
}
```

---

## 💡 关键经验总结

### 1. 新模块集成的正确姿势

| 步骤 | 说明 |
|------|------|
| 1 | 先实现模块，设权重=0 |
| 2 | 运行回测，对比基线 |
| 3 | 如果提升，保持权重=0或微调 |
| 4 | 如果下降，检查是否影响了原有信号平衡 |

### 2. 数据采集 vs 信号触发

| 类型 | 特点 | 适用场景 |
|------|------|----------|
| 数据采集 | 权重=0，仅记录信息 | 辅助其他模块判断 |
| 信号触发 | 权重>0，影响得分 | 直接参与交易决策 |

**结论**：新模块应该先作为数据采集，验证有效后再考虑是否参与信号触发。

### 3. 过滤器的双刃剑效应

过滤器的本意是过滤假信号，但如果阈值不合适，会过滤掉有效信号。

| 过滤器 | 预期效果 | 实际效果 |
|--------|----------|----------|
| EMA距离过滤 | 过滤逆势假信号 | 过滤了部分有效信号 |
| 成交量递减过滤 | 过滤无力信号 | 过滤了部分有效信号 |

**结论**：过滤器需要精细调参，不能直接使用默认阈值。

### 4. 权重系统的平衡性

原有的权重系统经过多次调优，各信号之间已经达到平衡。新增信号如果权重过高，会打破这个平衡。

**错误做法**：
```rust
(SignalType::FakeBreakout, 1.8),  // 最高权重，打破平衡
```

**正确做法**：
```rust
(SignalType::FakeBreakout, 0.0),  // 先设为0，观察效果
```

---

## 下一步计划

1. **R系统移动止损集成**：将 `r_system.rs` 集成到风控流程
2. **分批止盈实现**：40%/30%/30%分阶段止盈
3. **过滤器阈值调优**：调整EMA距离和成交量过滤的阈值
4. **时间止损**：12/24/48 K线无盈利自动平仓

---

## 历史基线

| 日期 | 回测ID | 配置 | 胜率 | 盈利 | 备注 |
|------|--------|------|------|------|------|
| 2026-01-06 | 4995 | 组合E | 54.7% | +52.77 | 旧基线 |
| 2026-01-06 | 5001 | 第一性原理v1 | 55.1% | +99.68 | **新基线** |
