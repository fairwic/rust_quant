# 如何添加新策略 - 快速指南

## 🚀 快速开始：3步添加新策略

通过策略注册中心架构，添加新策略变得极其简单！

---

## 📋 添加新策略流程

### Step 1: 创建策略执行器（唯一需要的文件）

**文件**: `src/trading/strategy/your_new_strategy_executor.rs`

```rust
//! YourNew 策略执行器
//! 
//! 封装 YourNew 策略的数据初始化和执行逻辑

use async_trait::async_trait;
use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use tracing::{debug, error, info, warn};

use super::strategy_trait::{StrategyDataResult, StrategyExecutor};
use crate::trading::domain_service::candle_domain_service::CandleDomainService;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::services::order_service::swap_order_service::SwapOrderService;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::{
    parse_candle_to_data_item, BasicRiskStrategyConfig, SignalResult,
};
use crate::trading::strategy::StrategyType;
use crate::trading::task::strategy_runner::{
    check_new_time, save_signal_log, StrategyExecutionStateManager,
};
use crate::CandleItem;
use okx::dto::EnumToStrTrait;

// 👇 导入你的策略配置和实现
use crate::trading::strategy::your_new_strategy::{
    YourNewStrategy, YourNewStrategyConfig, YourNewSignalValues,
};
use crate::trading::strategy::arc::indicator_values::arc_your_new_indicator_values::{
    self as arc_your_new, get_your_new_hash_key, get_your_new_indicator_manager,
};

/// YourNew 策略执行器
pub struct YourNewStrategyExecutor;

impl YourNewStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for YourNewStrategyExecutor {
    fn name(&self) -> &'static str {
        "YourNew"  // 👈 策略名称
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::YourNew  // 👈 策略类型
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        // 👇 尝试解析配置，判断是否为该策略类型
        serde_json::from_str::<YourNewStrategyConfig>(strategy_config).is_ok()
    }

    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandlesEntity>,
    ) -> Result<StrategyDataResult> {
        debug!("初始化 YourNew 策略数据: {}_{}", inst_id, period);

        // 1. 解析策略配置
        let your_new_config: YourNewStrategyConfig = 
            serde_json::from_str(&strategy_config.strategy_config)
                .map_err(|e| anyhow!("解析 YourNewStrategyConfig 失败: {}", e))?;

        // 2. 创建策略实例
        let your_new_strategy = YourNewStrategy::new(your_new_config.clone());
        let mut indicator_combine = your_new_strategy.get_indicator_combine();

        // 3. 转换K线数据并初始化指标
        let mut candle_items = VecDeque::with_capacity(candles.len());
        for candle in &candles {
            let data_item = parse_candle_to_data_item(candle);
            indicator_combine.next(&data_item);
            candle_items.push_back(data_item);
        }

        // 4. 获取最新时间戳
        let last_timestamp = candles
            .last()
            .ok_or_else(|| anyhow!("无法获取最新K线时间戳"))?
            .ts;

        // 5. 生成存储键
        let hash_key = get_your_new_hash_key(inst_id, period, StrategyType::YourNew.as_str());

        // 6. 存储到缓存
        arc_your_new::set_your_new_strategy_indicator_values(
            inst_id.to_string(),
            period.to_string(),
            last_timestamp,
            hash_key.clone(),
            candle_items,
            indicator_combine,
        )
        .await;

        // 7. 验证数据保存成功
        let manager = get_your_new_indicator_manager();
        if !manager.key_exists(&hash_key).await {
            return Err(anyhow!("YourNew 策略数据保存验证失败: {}", hash_key));
        }

        info!("✅ YourNew 策略数据初始化完成: {}", hash_key);

        Ok(StrategyDataResult {
            hash_key,
            last_timestamp,
        })
    }

    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<()> {
        const MAX_HISTORY_SIZE: usize = 10000;

        // 1. 获取策略类型和哈希键
        let strategy_type = StrategyType::YourNew.as_str().to_owned();
        let key = get_your_new_hash_key(inst_id, period, &strategy_type);
        let manager = get_your_new_indicator_manager();

        // 2. 获取最新K线数据
        let new_candle_data = if let Some(snap) = snap {
            snap
        } else {
            CandleDomainService::new_default()
                .await
                .get_new_one_candle_fresh(inst_id, period, None)
                .await
                .map_err(|e| anyhow!("获取最新K线数据失败: {}", e))?
                .ok_or_else(|| {
                    warn!("获取的最新K线数据为空: {:?}, {:?}", inst_id, period);
                    anyhow!("K线数据为空")
                })?
        };

        let new_candle_item = parse_candle_to_data_item(&new_candle_data);

        // 3. 获取互斥锁和缓存快照
        let key_mutex = manager.acquire_key_mutex(&key).await;
        let _guard = key_mutex.lock().await;

        let (last_candles_vec, mut old_indicator_combines, old_time) =
            match manager.get_snapshot_last_n(&key, MAX_HISTORY_SIZE).await {
                Some((v, indicators, ts)) => (v, indicators, ts),
                None => {
                    return Err(anyhow!("没有找到对应的 YourNew 策略值: {}", key));
                }
            };

        // 4. 转换为 VecDeque
        let mut new_candle_items: VecDeque<CandleItem> = 
            last_candles_vec.into_iter().collect();

        // 5. 验证时间戳
        let new_time = new_candle_item.ts;
        let is_update = new_candle_item.confirm == 1;

        let is_new_time = check_new_time(old_time, new_time, period, is_update, true)?;
        if !is_new_time {
            info!("跳过 YourNew 策略执行: inst_id={}, period={}", inst_id, period);
            return Ok(());
        }

        // 6. 去重检查
        if !StrategyExecutionStateManager::try_mark_processing(&key, new_candle_item.ts) {
            return Ok(());
        }

        // 7. 添加新K线
        new_candle_items.push_back(new_candle_item.clone());
        if new_candle_items.len() > MAX_HISTORY_SIZE {
            let excess = new_candle_items.len() - MAX_HISTORY_SIZE;
            for _ in 0..excess {
                new_candle_items.pop_front();
            }
        }

        // 8. 更新指标值
        let new_indicator_values = old_indicator_combines.next(&new_candle_item);

        // 9. 原子更新缓存
        if let Err(e) = manager
            .update_both(
                &key,
                new_candle_items.clone(),
                old_indicator_combines.clone(),
                new_candle_item.ts,
            )
            .await
        {
            return Err(anyhow!("原子更新 YourNew 指标与K线失败: {}", e));
        }

        // 10. 转换为切片（取最后10根K线）
        let candle_vec: Vec<CandleItem> = new_candle_items
            .iter()
            .rev()
            .take(10)
            .cloned()
            .rev()
            .collect();

        // 11. 解析策略配置并生成信号
        let your_new_config: YourNewStrategyConfig = 
            serde_json::from_str(&strategy_config.strategy_config)?;
        let mut your_new_strategy = YourNewStrategy::new(your_new_config);

        let signal_result = your_new_strategy.get_trade_signal(
            &candle_vec,
            &new_indicator_values,
        );

        info!(
            "YourNew 策略信号！inst_id={}, period={}, should_buy={}, should_sell={}, ts={}",
            inst_id, period, signal_result.should_buy, signal_result.should_sell, new_candle_item.ts
        );

        // 12. 如有信号则执行下单
        if signal_result.should_buy || signal_result.should_sell {
            // 记录信号日志
            save_signal_log(inst_id, period, &signal_result);

            // 解析风险配置
            let risk_config: BasicRiskStrategyConfig =
                serde_json::from_str(&strategy_config.risk_config)?;

            // 执行下单
            let res = SwapOrderService::new()
                .ready_to_order(
                    &StrategyType::YourNew,
                    inst_id,
                    period,
                    &signal_result,
                    &risk_config,
                    strategy_config.strategy_config_id,
                )
                .await;

            match res {
                Ok(_) => {
                    info!("✅ YourNew 策略下单成功");
                }
                Err(e) => {
                    error!("❌ YourNew 策略下单失败: {}", e);
                }
            }
        } else {
            debug!(
                "YourNew 策略: 无信号, ts={}",
                new_candle_items.back().unwrap().ts
            );
        }

        // 13. 清理执行状态
        StrategyExecutionStateManager::mark_completed(&key, new_candle_item.ts);

        Ok(())
    }
}
```

---

### Step 2: 在策略注册中心注册（1行代码）

**文件**: `src/trading/strategy/strategy_registry.rs`

找到 `initialize_registry()` 函数，添加一行：

```rust
fn initialize_registry() -> StrategyRegistry {
    use super::vegas_executor::VegasStrategyExecutor;
    use super::nwe_executor::NweStrategyExecutor;
    use super::your_new_strategy_executor::YourNewStrategyExecutor;  // 👈 导入
    
    let registry = StrategyRegistry::new();
    
    registry.register(Arc::new(VegasStrategyExecutor::new()));
    registry.register(Arc::new(NweStrategyExecutor::new()));
    registry.register(Arc::new(YourNewStrategyExecutor::new()));  // 👈 注册！
    
    info!(
        "🎯 策略注册中心初始化完成，已注册 {} 个策略: {:?}",
        registry.count(),
        registry.list_strategies()
    );
    
    registry
}
```

---

### Step 3: 导出模块

**文件**: `src/trading/strategy/mod.rs`

```rust
// 🆕 策略可扩展性框架
pub mod strategy_trait;
pub mod strategy_registry;
pub mod vegas_executor;
pub mod nwe_executor;
pub mod your_new_strategy_executor;  // 👈 添加这一行
```

---

## ✅ 完成！

**就这样！无需修改任何其他文件！**

- ❌ 无需修改 `strategy_runner.rs`
- ❌ 无需修改 `strategy_data_service.rs`
- ❌ 无需修改 `strategy_manager.rs`
- ❌ 无需添加 match 分支
- ❌ 无需修改检测逻辑

系统会自动：
1. 识别策略类型
2. 初始化数据
3. 执行策略
4. 生成信号
5. 执行下单

---

## 📚 前置条件

在创建执行器之前，你需要：

### 1. 策略配置结构
```rust
// src/trading/strategy/your_new_strategy/mod.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YourNewStrategyConfig {
    pub period: String,
    pub param1: usize,
    pub param2: f64,
    // ... 其他参数
}
```

### 2. 指标组合
```rust
#[derive(Debug, Clone)]
pub struct YourNewIndicatorCombine {
    pub indicator1: Option<Indicator1>,
    pub indicator2: Option<Indicator2>,
    // ... 其他指标
}

impl YourNewIndicatorCombine {
    pub fn next(&mut self, candle: &CandleItem) -> YourNewSignalValues {
        // 推进所有指标并返回值
    }
}
```

### 3. 指标缓存管理器
```rust
// src/trading/strategy/arc/indicator_values/arc_your_new_indicator_values.rs

// 复制 arc_nwe_indicator_values.rs 并修改类型名称
```

### 4. 策略枚举类型
```rust
// src/trading/strategy/mod.rs

#[derive(Clone, Copy, Debug)]
pub enum StrategyType {
    // ...
    YourNew,  // 👈 添加新类型
}

impl StrategyType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            // ...
            "YourNew" => Some(StrategyType::YourNew),  // 👈 添加映射
            _ => None,
        }
    }
}

impl EnumToStrTrait for StrategyType {
    fn as_str(&self) -> &'static str {
        match self {
            // ...
            StrategyType::YourNew => "YourNew",  // 👈 添加映射
        }
    }
}
```

---

## 🎯 完整示例：添加 MACD 策略

### 准备工作

1. **策略配置** - `src/trading/strategy/macd_strategy/mod.rs`
2. **指标组合** - `src/trading/strategy/macd_strategy/indicator_combine.rs`
3. **缓存管理器** - `src/trading/strategy/arc/indicator_values/arc_macd_indicator_values.rs`
4. **枚举类型** - 在 `StrategyType` 添加 `Macd` 变体

### 核心代码

```rust
// src/trading/strategy/macd_executor.rs

pub struct MacdStrategyExecutor;

impl MacdStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for MacdStrategyExecutor {
    fn name(&self) -> &'static str {
        "Macd"
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::Macd
    }

    fn can_handle(&self, strategy_config: &str) -> bool {
        serde_json::from_str::<MacdStrategyConfig>(strategy_config).is_ok()
    }

    async fn initialize_data(&self, ...) -> Result<StrategyDataResult> {
        // 参考 Nwe/Vegas 实现
    }

    async fn execute(&self, ...) -> Result<()> {
        // 参考 Nwe/Vegas 实现
    }
}
```

### 注册策略

```rust
// src/trading/strategy/strategy_registry.rs

fn initialize_registry() -> StrategyRegistry {
    use super::vegas_executor::VegasStrategyExecutor;
    use super::nwe_executor::NweStrategyExecutor;
    use super::macd_executor::MacdStrategyExecutor;  // 导入
    
    let registry = StrategyRegistry::new();
    
    registry.register(Arc::new(VegasStrategyExecutor::new()));
    registry.register(Arc::new(NweStrategyExecutor::new()));
    registry.register(Arc::new(MacdStrategyExecutor::new()));  // 注册
    
    registry
}
```

### 导出模块

```rust
// src/trading/strategy/mod.rs

pub mod macd_executor;  // 导出
```

---

## 📊 工作量对比

| 操作 | 旧架构 | 新架构 ⭐ |
|------|--------|----------|
| **创建执行器** | - | 1 个文件 |
| **修改 strategy_runner** | ✅ 必须 | ❌ 无需 |
| **修改 strategy_data_service** | ✅ 必须 | ❌ 无需 |
| **修改 detect_strategy_type** | ✅ 必须 | ❌ 无需 |
| **添加 match 分支** | ✅ 必须 | ❌ 无需 |
| **注册策略** | - | 1 行代码 |
| **导出模块** | ✅ 必须 | ✅ 必须 |
| **总修改文件数** | 6+ | 3 |
| **总代码行数** | 300+ | 50+ |

**工作量减少 85%！** 🎉

---

## ⚡ 最佳实践

### 1. 命名规范
- 执行器文件: `{strategy_name}_executor.rs`
- 执行器结构: `{StrategyName}StrategyExecutor`
- 策略名称: 与 `StrategyType` 枚举保持一致

### 2. 代码复用
- 复制 `nwe_executor.rs` 作为模板
- 替换策略相关的类型和逻辑
- 保持执行流程一致

### 3. 测试
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let executor = YourNewStrategyExecutor::new();
        let config = serde_json::to_string(&YourNewStrategyConfig::default()).unwrap();
        assert!(executor.can_handle(&config));
    }
}
```

---

## 🔍 调试技巧

### 查看已注册策略
```rust
use crate::trading::strategy::strategy_registry::get_strategy_registry;

let registry = get_strategy_registry();
println!("已注册策略: {:?}", registry.list_strategies());
```

### 手动获取策略
```rust
let strategy = registry.get("YourNew")?;
strategy.execute(...).await?;
```

---

## 🎓 进阶用法

### 策略热重载
```rust
// 移除旧策略
registry.unregister("YourNew");

// 重新注册新版本
registry.register(Arc::new(YourNewStrategyExecutorV2::new()));
```

### 动态禁用策略
```rust
// 在注册前检查配置
if config.enable_your_new_strategy {
    registry.register(Arc::new(YourNewStrategyExecutor::new()));
}
```

---

## 📋 检查清单

新增策略时，确保：

- [ ] 创建策略配置结构（`YourNewStrategyConfig`）
- [ ] 实现指标组合（`YourNewIndicatorCombine`）
- [ ] 创建指标缓存管理器（`arc_your_new_indicator_values.rs`）
- [ ] 在 `StrategyType` 添加新枚举变体
- [ ] 创建策略执行器（`your_new_strategy_executor.rs`）
- [ ] 在注册中心注册（1行代码）
- [ ] 导出模块（`mod.rs`）
- [ ] 编译测试
- [ ] 单元测试
- [ ] 实盘测试

---

## 🎉 总结

**新架构优势**：

✅ **开闭原则** - 对扩展开放，对修改关闭  
✅ **单一职责** - 每个策略独立封装  
✅ **依赖注入** - 通过注册中心管理  
✅ **类型安全** - 编译时保证  
✅ **易于测试** - 每个策略独立测试  
✅ **可维护性** - 代码清晰，易于理解  

**添加新策略仅需 3 步！** 🚀

---

**文档版本**: v1.0  
**最后更新**: 2025-10-28  
**作者**: AI Assistant

