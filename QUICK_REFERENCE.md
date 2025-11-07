# 快速参考卡片

## 📦 可用的包 (7/14)

```
✅ rust-quant-common         公共类型和工具
✅ rust-quant-core           配置、日志、数据库
✅ rust-quant-domain         领域模型（纯业务）
✅ rust-quant-infrastructure 数据访问、缓存
✅ rust-quant-indicators     技术指标计算
✅ rust-quant-market         市场数据
✅ rust-quant-ai-analysis    AI分析
```

---

## 🚀 立即使用

### 使用域模型
```rust
use rust_quant_domain::{StrategyType, Timeframe, SignalResult};
```

### 使用指标
```rust
use rust_quant_indicators::trend::nwe::{
    NweIndicatorCombine, NweIndicatorConfig
};

let mut combine = NweIndicatorCombine::new(&config);
let values = combine.next(&candle);
```

### 使用适配器
```rust
use rust_quant_strategies::adapters::candle_adapter;

let adapter = candle_adapter::adapt(&candle);
let high = adapter.high();
```

### 访问数据
```rust
use rust_quant_infrastructure::SqlxCandleRepository;

let repo = SqlxCandleRepository::new(pool);
let candles = repo.find_candles(...).await?;
```

---

## 📚 文档导航

| 需求 | 文档 |
|------|------|
| 快速修复问题 | `ON_DEMAND_FIX_GUIDE.md` ⭐ |
| 了解架构 | `ARCHITECTURE_REFACTORING_PLAN_V2.md` |
| 查看进度 | `PHASE2_COMPLETION_SUMMARY.md` |
| 剩余工作 | `REMAINING_WORK_ANALYSIS.md` |
| 快速参考 | `QUICK_REFERENCE.md` (本文档) |

---

## 🎯 最佳实践参考

| 场景 | 参考文件 |
|------|----------|
| 解决孤儿规则 | `strategies/src/adapters/candle_adapter.rs` |
| 指标组合 | `indicators/src/trend/nwe/indicator_combine.rs` |
| 策略实现 | `strategies/src/implementations/nwe_strategy/mod.rs` |
| 单元测试 | 上述文件的测试部分 |

---

## ⚡ 常见场景

### 场景 1: StrategyConfig 字段错误
```rust
// ❌ config.strategy_config_id
// ✅ config.id

// ❌ config.strategy_config
// ✅ config.parameters
```

### 场景 2: 类型转换
```rust
// 提取参数
let params: MyConfig = serde_json::from_value(
    config.parameters.clone()
)?;
```

### 场景 3: 孤儿规则
```rust
// 创建本地wrapper
pub struct MyAdapter(ExternalType);
impl ExternalTrait for MyAdapter { }
```

---

## 📊 包依赖关系

```
cli
 ↓
orchestration
 ↓
strategies ─────┐
 ↓              ↓
infrastructure  indicators
 ↓              ↓
domain       common
 ↓              ↓
core ←──────────┘
```

**规则**: 只能从上到下依赖 ✅

---

## 🔧 开发命令

```bash
# 编译单个包
cargo build -p rust-quant-xxx

# 编译所有包
cargo build --workspace

# 运行测试
cargo test -p rust-quant-xxx

# 查看错误
cargo build 2>&1 | grep "error\["

# 统计错误
cargo build 2>&1 | grep "error\[" | wc -l
```

---

## 💡 关键提示

1. **遇到错误先查** `ON_DEMAND_FIX_GUIDE.md`
2. **创建新功能参考** 已完成的模块
3. **遵循架构规范** 不要跨层依赖
4. **小步骤迭代** 频繁编译验证

---

**下一步**: 根据需要开发，遇到问题查文档 ✅


