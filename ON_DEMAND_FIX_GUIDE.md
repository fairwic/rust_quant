# 按需修复指南

## 🎯 使用说明

当你在开发中遇到问题时，在这里查找对应的修复方案。

---

## 📚 快速索引

1. [如何使用已完成的7个包](#使用已完成的包)
2. [如果需要修复 strategies 包](#修复-strategies-包)
3. [如果遇到孤儿规则错误](#孤儿规则错误)
4. [如果需要创建新指标](#创建新指标)
5. [如果需要创建新策略](#创建新策略)
6. [常见编译错误快速修复](#常见编译错误)

---

## ✅ 使用已完成的包

### 1. 使用 domain 包

**场景**: 需要使用业务实体和枚举

```rust
use rust_quant_domain::{
    StrategyType,
    StrategyStatus,
    Timeframe,
    SignalResult,
    TradingSignal,
};

// 使用枚举
let strategy_type = StrategyType::Vegas;
let timeframe = Timeframe::H1;
```

### 2. 使用 infrastructure 包

**场景**: 需要访问数据库或缓存

```rust
use rust_quant_infrastructure::{
    SqlxCandleRepository,
    StrategyConfigEntityModel,
    cache::arc_vegas_indicator_values,
};

// 使用仓储
let repo = SqlxCandleRepository::new(pool);
let candles = repo.find_candles(...).await?;

// 使用缓存
arc_vegas_indicator_values::set_strategy_indicator_values(...).await;
```

### 3. 使用 indicators 包

**场景**: 需要计算技术指标

```rust
use rust_quant_indicators::{
    momentum::RSI,
    trend::EMA,
    trend::nwe::{NweIndicatorCombine, NweIndicatorConfig},
};

// 使用 NWE 指标组合
let config = NweIndicatorConfig::default();
let mut combine = NweIndicatorCombine::new(&config);
let values = combine.next(&candle);
```

### 4. 使用适配器解决孤儿规则

**场景**: 需要为外部类型实现外部 trait

```rust
use rust_quant_strategies::adapters::candle_adapter;
use ta::{High, Low, Close};

// 使用适配器
let adapter = candle_adapter::adapt(&candle);
let high = adapter.high();
let low = adapter.low();
let close = adapter.close();

// 批量转换
let adapters = candle_adapter::adapt_many(&candles);
```

---

## 🔧 修复 Strategies 包

### 场景 1: StrategyConfig 字段不存在

**错误信息**:
```
error: struct `StrategyConfig` has no field named `strategy_config_id`
error: struct `StrategyConfig` has no field named `strategy_config`
```

**原因**: StrategyConfig 结构已更新

**快速修复**:
```rust
// ❌ 旧代码
config.strategy_config_id
config.strategy_config

// ✅ 新代码
config.id
config.parameters  // 这是 JsonValue 类型
```

**提取参数的辅助函数**:
```rust
// strategies/src/framework/config/mod.rs 中添加

use anyhow::Result;
use serde_json::Value as JsonValue;

/// 从 StrategyConfig 提取策略参数
pub fn extract_parameters<T: serde::de::DeserializeOwned>(
    config: &StrategyConfig
) -> Result<T> {
    serde_json::from_value(config.parameters.clone())
        .map_err(|e| anyhow::anyhow!("Failed to extract parameters: {}", e))
}

/// 从 StrategyConfig 提取风险配置
pub fn extract_risk_config<T: serde::de::DeserializeOwned>(
    config: &StrategyConfig
) -> Result<T> {
    serde_json::from_value(config.risk_config.clone())
        .map_err(|e| anyhow::anyhow!("Failed to extract risk_config: {}", e))
}

// 使用
let vegas_config: VegasStrategyConfig = extract_parameters(&strategy_config)?;
let risk_config: BasicRiskConfig = extract_risk_config(&strategy_config)?;
```

### 场景 2: risk_config 类型错误

**错误信息**:
```
error: expected `Value`, found `String`
```

**快速修复**:
```rust
// ❌ 旧代码
risk_config: serde_json::to_string(&risk_config).unwrap()

// ✅ 新代码
risk_config: serde_json::json!(risk_config)
```

### 场景 3: 构造 StrategyConfig

**新的构造方式**:
```rust
use rust_quant_domain::StrategyConfig;
use chrono::Utc;

let strategy_config = StrategyConfig {
    id: strategy_config_id,
    strategy_type: StrategyType::Vegas,
    symbol: "BTC-USDT".to_string(),
    timeframe: Timeframe::H1,
    parameters: serde_json::json!(vegas_strategy),
    risk_config: serde_json::json!(risk_config),
    status: StrategyStatus::Stopped,
    created_at: Utc::now(),
    updated_at: Utc::now(),
    backtest_start: None,
    backtest_end: None,
    description: None,
};
```

---

## 🚫 孤儿规则错误

### 场景: 为外部类型实现外部 trait

**错误信息**:
```
error[E0117]: only traits defined in the current crate can be implemented 
              for types defined outside of the crate
```

**解决方案**: 使用适配器模式

**步骤 1**: 创建本地包装类型
```rust
// 在你的 crate 中
pub struct MyAdapter {
    pub data: ExternalType,
}

impl From<&ExternalType> for MyAdapter {
    fn from(external: &ExternalType) -> Self {
        Self { data: external.clone() }
    }
}
```

**步骤 2**: 为包装类型实现 trait
```rust
impl ExternalTrait for MyAdapter {
    fn method(&self) -> Result {
        // 实现
    }
}
```

**步骤 3**: 提供便捷函数
```rust
pub fn adapt(external: &ExternalType) -> MyAdapter {
    MyAdapter::from(external)
}
```

**参考**: `strategies/src/adapters/candle_adapter.rs`

---

## 📊 创建新指标

### 场景: 添加新的技术指标

**步骤 1**: 确定指标类型
- 趋势指标 → `indicators/src/trend/`
- 动量指标 → `indicators/src/momentum/`
- 波动率指标 → `indicators/src/volatility/`
- 成交量指标 → `indicators/src/volume/`
- 形态识别 → `indicators/src/pattern/`

**步骤 2**: 创建指标文件
```rust
// indicators/src/trend/my_indicator.rs
use rust_quant_common::CandleItem;

pub struct MyIndicator {
    period: usize,
    // 内部状态
}

impl MyIndicator {
    pub fn new(period: usize) -> Self {
        Self { period }
    }
    
    pub fn next(&mut self, price: f64) -> f64 {
        // 计算逻辑
        0.0
    }
}
```

**步骤 3**: 导出指标
```rust
// indicators/src/trend/mod.rs
pub mod my_indicator;
pub use my_indicator::*;
```

**步骤 4**: 添加测试
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_my_indicator() {
        let mut indicator = MyIndicator::new(14);
        let result = indicator.next(100.0);
        assert!(result.is_finite());
    }
}
```

**参考**: `indicators/src/trend/nwe/indicator_combine.rs`

---

## 🎯 创建新策略

### 场景: 实现新的交易策略

**步骤 1**: 在 strategies 包中创建策略文件
```rust
// strategies/src/implementations/my_strategy.rs
use rust_quant_domain::{SignalResult, TradingSignal};
use rust_quant_common::CandleItem;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyStrategyConfig {
    pub period: usize,
    pub threshold: f64,
}

pub struct MyStrategy {
    config: MyStrategyConfig,
}

impl MyStrategy {
    pub fn new(config: MyStrategyConfig) -> Self {
        Self { config }
    }
    
    pub fn analyze(&self, candles: &[CandleItem]) -> SignalResult {
        // 策略逻辑
        SignalResult {
            should_buy: false,
            should_sell: false,
            // ... 其他字段
        }
    }
}
```

**步骤 2**: 使用指标
```rust
use rust_quant_indicators::trend::EMA;

impl MyStrategy {
    pub fn analyze(&self, candles: &[CandleItem]) -> SignalResult {
        // 使用指标
        let mut ema = EMA::new(self.config.period);
        let ema_value = ema.next(candles.last().unwrap().c);
        
        // 生成信号
        SignalResult {
            should_buy: ema_value > self.config.threshold,
            should_sell: ema_value < self.config.threshold,
            // ...
        }
    }
}
```

**步骤 3**: 导出策略
```rust
// strategies/src/implementations/mod.rs
pub mod my_strategy;
pub use my_strategy::*;
```

**参考**: `strategies/src/implementations/nwe_strategy/mod.rs`

---

## ⚡ 常见编译错误快速修复

### 错误 1: 字段私有

**错误**:
```
error[E0616]: field `k` of struct `KDJ` is private
```

**修复**: 添加 getter 方法
```rust
// indicators 包中
impl KDJ {
    pub fn k(&self) -> f64 { self.k }
    pub fn d(&self) -> f64 { self.d }
    pub fn j(&self) -> f64 { self.j }
}

// 使用
kdj.k()  // 而不是 kdj.k
```

### 错误 2: 循环依赖

**错误**:
```
error: cyclic package dependency
```

**原因**: 包之间的依赖形成了环

**修复**: 遵循依赖方向规则
```
正确的依赖方向 (单向):
orchestration → strategies
strategies → indicators
strategies → infrastructure
infrastructure → domain
indicators → domain
```

**不允许**:
- strategies 依赖 orchestration ❌
- domain 依赖任何业务包 ❌

### 错误 3: 模块找不到

**错误**:
```
error[E0432]: unresolved import `crate::xxx`
```

**修复步骤**:
1. 检查模块是否存在
2. 检查 `mod.rs` 是否导出
3. 检查 `lib.rs` 是否声明

```rust
// lib.rs
pub mod my_module;

// mod.rs
pub mod submodule;
pub use submodule::*;
```

### 错误 4: 方法不存在

**错误**:
```
error[E0599]: no method named `xxx` found
```

**临时方案**: 注释掉调用
```rust
// ❌ 如果方法不存在
// result.method_that_does_not_exist();

// ✅ 临时注释
// TODO: 实现或找到替代方法
```

---

## 🛠️ 实用工具命令

### 查找特定错误
```bash
# 查找字段访问
grep -r "strategy_config_id" crates/strategies/

# 查找类型使用
grep -r "BasicRiskStrategyConfig" crates/strategies/

# 查找导入
grep -r "use.*orchestration" crates/strategies/
```

### 批量替换
```bash
# 替换字段名
find crates/strategies/src -name "*.rs" -type f \
  -exec sed -i.bak 's/strategy_config_id/id/g' {} \;

# 清理备份文件
find crates/strategies/src -name "*.bak" -type f -delete
```

### 编译特定包
```bash
# 只编译 strategies
cargo build -p rust-quant-strategies

# 查看详细错误
cargo build -p rust-quant-strategies 2>&1 | less

# 统计错误数
cargo build -p rust-quant-strategies 2>&1 | grep "error\[" | wc -l
```

---

## 📚 最佳实践参考

### 1. 适配器模式实现
**文件**: `strategies/src/adapters/candle_adapter.rs`  
**用途**: 解决孤儿规则问题

### 2. 指标组合实现
**文件**: `indicators/src/trend/nwe/indicator_combine.rs`  
**用途**: 组合多个指标的标准方式

### 3. 策略实现
**文件**: `strategies/src/implementations/nwe_strategy/mod.rs`  
**用途**: 策略结构和逻辑组织

### 4. 单元测试
**位置**: 各模块的 `#[cfg(test)] mod tests`  
**用途**: 测试编写参考

---

## 🔍 问题诊断流程

### 遇到编译错误时

1. **看错误类型**
   - E0117: 孤儿规则 → 使用适配器
   - E0432: 导入错误 → 检查模块路径
   - E0560: 字段不存在 → 查看结构定义
   - E0616: 字段私有 → 添加 getter

2. **定位问题文件**
   ```bash
   cargo build 2>&1 | grep "error\[" | head -10
   ```

3. **查找相似代码**
   - 在已完成的包中查找类似实现
   - 参考本指南的示例

4. **小步骤修复**
   - 一次修复一个错误
   - 及时编译验证
   - 提交可工作的版本

---

## 📊 包使用优先级

### 高优先级 (立即可用)
```
✅ rust-quant-common         - 公共类型和工具
✅ rust-quant-core           - 配置和日志
✅ rust-quant-domain         - 领域模型
✅ rust-quant-infrastructure - 数据访问和缓存
✅ rust-quant-indicators     - 技术指标
✅ rust-quant-market         - 市场数据
✅ rust-quant-ai-analysis    - AI分析
```

### 中优先级 (部分可用)
```
🟡 rust-quant-strategies     - 部分策略可用
   可用: nwe_strategy, engulfing_strategy, macd_kdj_strategy
   需修复: framework/strategy_manager
```

### 低优先级 (按需修复)
```
⏸️  rust-quant-orchestration  - 任务调度
⏸️  rust-quant-execution      - 订单执行
⏸️  rust-quant-risk           - 风险管理
⏸️  rust-quant-analytics      - 分析报告
⏸️  rust-quant-services       - 应用服务
⏸️  rust-quant-cli            - 命令行
```

---

## 💡 开发建议

### 新功能开发

1. **优先使用已完成的包**
   - 使用 indicators 开发新指标
   - 使用 domain 定义新实体
   - 使用 infrastructure 访问数据

2. **遇到问题再修复**
   - 不要提前修复所有问题
   - 根据实际需求修复
   - 保持迭代开发

3. **参考现有代码**
   - 适配器模式: `adapters/candle_adapter.rs`
   - 指标组合: `indicators/trend/nwe/`
   - 策略实现: `strategies/implementations/nwe_strategy/`

### 代码组织

1. **遵循包职责**
   - indicators: 纯计算
   - strategies: 决策逻辑
   - infrastructure: 数据访问
   - domain: 业务模型

2. **依赖方向**
   ```
   上层 → 下层 (单向)
   不允许反向依赖
   ```

3. **测试驱动**
   - 新代码带测试
   - 参考现有测试

---

## 📞 获取帮助

### 查阅文档
- **架构设计**: `ARCHITECTURE_REFACTORING_PLAN_V2.md`
- **当前状态**: `FINAL_PHASE2_STATUS.md`
- **完成总结**: `PHASE2_COMPLETION_SUMMARY.md`
- **剩余分析**: `REMAINING_WORK_ANALYSIS.md`
- **本指南**: `ON_DEMAND_FIX_GUIDE.md`

### 常见问题
1. 孤儿规则 → 参考适配器模式
2. 字段不匹配 → 参考 StrategyConfig 修复
3. 循环依赖 → 检查依赖方向
4. 模块找不到 → 检查导出

---

## 🎯 总结

**核心原则**: 按需修复，迭代开发

**可用资源**:
- ✅ 7个完全可用的包
- ✅ 完整的文档体系
- ✅ 清晰的代码示例
- ✅ 本按需修复指南

**开发流程**:
1. 使用已完成的包开发
2. 遇到问题查本指南
3. 参考示例代码
4. 小步骤迭代

**成功标准**: 
- 实现你需要的功能
- 而不是修复所有错误

---

*最后更新: 2025-11-07*  
*版本: v1.0*  
*适用于: Rust Quant v0.2.1*


