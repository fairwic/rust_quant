# 震荡结束突破下跌策略 - 快速开始

## 当前状态

✅ **代码完成** - 策略核心逻辑、回测适配器、单元测试全部实现并通过  
⚠️ **需要真实数据** - 缺乏历史K线数据验证策略有效性

## 快速测试

### 1. 运行单元测试（验证代码正确性）

```bash
cargo test -p rust-quant-strategies --lib range_breakout_drop::strategy::tests -- --nocapture
```

**预期结果**: 3个测试全部通过 ✅

### 2. 运行回测（当前使用模拟数据）

```bash
cargo test -p rust-quant-strategies range_breakout_drop_final_optimization -- --nocapture --ignored
```

**当前结果**: 不产生交易（模拟数据不够真实）⚠️

## 下一步：加载真实数据

### 方案A: 从数据库加载（推荐）

1. 设置数据库连接：
```bash
export QUANT_CORE_DATABASE_URL="postgres://user:pass@host:5432/quant_core"
```

2. 运行真实数据回测：
```bash
cargo run --bin range_breakout_drop_backtest
```

### 方案B: 从CSV加载

1. 准备CSV文件（格式：ts,o,h,l,c,v,confirm）
2. 修改测试加载CSV
3. 运行回测

### 方案C: 使用现有回测脚本

查看 `crates/rust-quant-cli/src/bin/` 中的其他回测脚本，参考它们如何加载数据。

## 策略说明

**目标**: 捕捉震荡区间结束后向下突破的做空机会

**核心逻辑**:
1. 识别震荡区间（过去N根K线波动在一定范围内）
2. 检测突破（价格突破震荡下边界）
3. 确认质量（实体比例、移动距离、成交量）
4. 做空入场

**风险管理**:
- 止损：震荡上边界 + 1.5 ATR
- 止盈：1R / 2R / 3.5R 三档

## 文件结构

```
crates/strategies/src/implementations/range_breakout_drop/
├── mod.rs              # 模块导出
├── types.rs            # 数据结构（Thresholds, Snapshot, Tuning）
├── strategy.rs         # 核心逻辑 + 回测适配器 + 单元测试
├── executor.rs         # 实盘执行器框架
├── README.md          # 策略文档
└── DEVELOPMENT_SUMMARY.md  # 开发过程记录

tests/
├── range_breakout_drop_optimization.rs  # 参数优化测试
└── ... (其他调试测试)

docs/
└── RANGE_BREAKOUT_DROP_COMPLETION_REPORT.md  # 完整开发报告
```

## 参数调整

如果真实数据回测仍然不产生交易，尝试逐步放宽参数：

```rust
let mut tuning = RangeBreakoutDropBacktestTuning::default();

// 第一轮：放宽震荡识别
tuning.max_range_volatility_pct = 4.0;  // 从3.0提高到4.0
tuning.min_range_volatility_pct = 0.3;  // 从0.5降低到0.3

// 第二轮：降低突破要求
tuning.min_breakout_body_ratio = 0.4;   // 从0.55降到0.4
tuning.min_breakout_move_atr = 0.5;     // 从0.8降到0.5
tuning.min_breakout_volume_mult = 1.2;  // 从1.5降到1.2

// 第三轮：关闭过滤器
tuning.require_bearish_ema = false;      // 关闭EMA过滤
tuning.rsi_min_before_drop = 25.0;      // 降低RSI限制
```

## 评估标准

当策略产生交易后，评估以下指标：

- ✅ **可行性**: 是否产生足够的交易信号？（每月 > 3笔）
- ✅ **胜率**: 盈利交易比例 > 50%？
- ✅ **盈亏比**: 平均盈利 / 平均亏损 > 1.2？
- ✅ **总盈亏**: 是否为正？
- ✅ **最大回撤**: < 10%？

如果全部满足 → 进入参数优化阶段  
如果部分满足 → 调整策略逻辑  
如果全不满足 → 考虑放弃该策略

## 常见问题

### Q: 为什么模拟数据不产生交易？
A: 因为突破判定条件严格，需要：价格真正突破下边界 + 阴线 + 足够的实体比例 + 足够的移动距离 + 放量。手工生成的数据很难同时满足所有条件。

### Q: 如何知道参数是否合理？
A: 只能在真实历史数据上回测。参数过严 = 无交易，参数过松 = 交易质量差。

### Q: 策略能盈利吗？
A: 未知。必须在真实数据上验证。量化策略开发就是：设计→实现→验证→优化的循环，当前停在"验证"阶段。

## 联系与支持

如有问题或需要协助，请查看：
- 完整开发报告：`docs/RANGE_BREAKOUT_DROP_COMPLETION_REPORT.md`
- 策略文档：`crates/strategies/src/implementations/range_breakout_drop/README.md`
- 开发过程：`crates/strategies/src/implementations/range_breakout_drop/DEVELOPMENT_SUMMARY.md`
