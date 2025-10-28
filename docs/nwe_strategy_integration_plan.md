# NweStrategy 实盘策略集成方案

## 一、实盘策略下单流程分析

### 1.1 核心流程架构

```
启动阶段（strategy_manager.start_strategy）
  ↓
数据初始化（StrategyDataService）
  ↓
定时任务注册（SchedulerService）
  ↓
K线确认触发（CandleService/WebSocket）
  ↓
策略执行（run_ready_to_order_with_manager）
  ↓
信号生成（Strategy.get_trade_signal）
  ↓
订单执行（SwapOrderService.ready_to_order）
```

### 1.2 关键模块说明

#### 📦 StrategyManager (src/trading/strategy/strategy_manager.rs)
**职责**: 策略生命周期管理
- `start_strategy()`: 启动策略
- `stop_strategy()`: 停止策略
- `run_ready_to_order_with_manager()`: 策略执行入口

#### 📦 StrategyDataService (src/trading/services/strategy_data_service.rs)
**职责**: 策略数据初始化和管理
- `initialize_strategy_data()`: 初始化K线和指标数据
- 当前 **仅支持 Vegas 策略** ⚠️

#### 📦 StrategyRunner (src/trading/task/strategy_runner.rs)
**职责**: 实盘策略执行逻辑
- `run_ready_to_order_with_manager()`: **硬编码 Vegas 策略** ⚠️
  - 第 583 行: `let strategy_type = StrategyType::Vegas.as_str().to_owned();`
  - 第 585 行: `let manager = arc_vegas_indicator_values::get_indicator_manager();`
  - 第 676-684 行: 直接调用 `vegas_strategy.get_trade_signal()`
  - 第 700 行: `&StrategyType::Vegas` 传递给订单服务

#### 📦 ArcVegasIndicatorValues (src/trading/strategy/arc/indicator_values/)
**职责**: Vegas策略指标值缓存管理
- `IndicatorValuesManager`: 指标值存储和更新
- `get_hash_key()`: 生成唯一键
- `update_both()`: 原子更新K线和指标

#### 📦 SwapOrderService (src/trading/services/order_service/)
**职责**: 实盘下单执行
- `ready_to_order()`: 执行下单逻辑
- 需要传入策略类型参数

---

## 二、当前 Vegas 策略流程详解

### 2.1 启动流程

```rust
// 1. 加载配置 (strategy_manager.rs:492)
let (config_entity, strategy_config) = 
    self.load_strategy_config(strategy_config_id).await?;

// 2. 初始化数据 (strategy_manager.rs:520)
let _data_snapshot = StrategyDataService::initialize_strategy_data(
    &strategy_config_for_init,
    &inst_id,
    &period,
).await?;
// 内部实现:
//   - 获取7000根历史K线
//   - 解析策略配置
//   - 计算初始指标值
//   - 存储到 arc_vegas_indicator_values

// 3. 创建定时任务 (strategy_manager.rs:539)
let scheduled_job = SchedulerService::create_scheduled_job(
    inst_id.clone(),
    period.clone(),
    config_entity.strategy_type.clone(),
    shared_config.clone(),
)?;
```

### 2.2 执行流程

```rust
// 1. K线确认触发 (candle_service.rs:66)
if snap.confirm == "1" {
    strategy_manager.run_ready_to_order_with_manager(
        &inst_id_owned,
        &time_interval_owned,
        Some(snap),
    ).await?;
}

// 2. 获取指标缓存 (strategy_runner.rs:614)
let (mut last_candles_vec, mut old_indicator_combines, old_time) =
    match manager.get_snapshot_last_n(&key, MAX_HISTORY_SIZE).await {
        Some((v, indicators, ts)) => (v, indicators, ts),
        None => return Err(anyhow!("没有找到对应的策略值: {}", key)),
    };

// 3. 更新指标值 (strategy_runner.rs:646-666)
let new_indicator_values = get_multi_indicator_values(
    &mut new_candle_items,
    old_indicator_combines,
);

// 4. 生成交易信号 (strategy_runner.rs:676-684)
let vegas_strategy: VegasStrategy = 
    serde_json::from_str(&strategy.strategy_config)?;
let signal_result = vegas_strategy.get_trade_signal(
    &candle_vec,
    &mut new_indicator_values.clone(),
    &SignalWeightsConfig::default(),
    &risk_config,
);

// 5. 执行下单 (strategy_runner.rs:698-709)
if signal_result.should_buy || signal_result.should_sell {
    SwapOrderService::new()
        .ready_to_order(
            &StrategyType::Vegas,
            inst_id,
            period,
            &signal_result,
            &risk_config,
            strategy.strategy_config_id,
        )
        .await?;
}
```

---

## 三、NweStrategy 集成修改清单

### 🔴 **问题 1**: strategy_runner.rs 硬编码 Vegas 策略

**文件**: `src/trading/task/strategy_runner.rs`
**位置**: 第 573-730 行

#### 当前实现缺陷:
```rust
// ❌ 硬编码 1: 策略类型
let strategy_type = StrategyType::Vegas.as_str().to_owned();  // Line 583

// ❌ 硬编码 2: 指标管理器
let manager = arc_vegas_indicator_values::get_indicator_manager();  // Line 585

// ❌ 硬编码 3: 策略解析和信号生成
let vegas_strategy: VegasStrategy = 
    serde_json::from_str(&strategy.strategy_config)?;  // Line 676

// ❌ 硬编码 4: 订单服务调用
SwapOrderService::new().ready_to_order(
    &StrategyType::Vegas,  // Line 700
    ...
)
```

#### ✅ 解决方案: 策略类型识别和动态分发

```rust
/// 运行准备好的订单函数 - 支持多策略类型
pub async fn run_ready_to_order_with_manager(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
    snap: Option<CandlesEntity>,
) -> Result<()> {
    // 1. 从配置解析策略类型
    let strategy_type = detect_strategy_type(&strategy.strategy_config)?;
    
    // 2. 根据策略类型分发到不同处理函数
    match strategy_type {
        StrategyType::Vegas => {
            run_vegas_strategy(inst_id, period, strategy, snap).await
        }
        StrategyType::Nwe => {
            run_nwe_strategy(inst_id, period, strategy, snap).await
        }
        _ => Err(anyhow!("不支持的策略类型: {:?}", strategy_type))
    }
}

/// Vegas 策略执行（保持原逻辑）
async fn run_vegas_strategy(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
    snap: Option<CandlesEntity>,
) -> Result<()> {
    // 原 run_ready_to_order_with_manager 的 Vegas 部分代码
    // ...
}

/// Nwe 策略执行（新增）
async fn run_nwe_strategy(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
    snap: Option<CandlesEntity>,
) -> Result<()> {
    // 参照 Vegas 实现
    // ...
}

/// 检测策略类型的辅助函数
fn detect_strategy_type(strategy_config: &str) -> Result<StrategyType> {
    // 尝试解析 VegasStrategy
    if serde_json::from_str::<VegasStrategy>(strategy_config).is_ok() {
        return Ok(StrategyType::Vegas);
    }
    // 尝试解析 NweStrategyConfig
    if serde_json::from_str::<NweStrategyConfig>(strategy_config).is_ok() {
        return Ok(StrategyType::Nwe);
    }
    Err(anyhow!("无法识别策略类型"))
}
```

---

### 🟡 **问题 2**: 缺少 NweIndicatorValuesManager

**需要**: 创建 Nwe 策略的指标缓存管理器

#### 📂 新建文件: `src/trading/strategy/arc/indicator_values/arc_nwe_indicator_values.rs`

```rust
//! Nwe 策略指标值缓存管理器
//! 参考 arc_vegas_indicator_values.rs 的设计

use crate::trading::strategy::nwe_strategy::indicator_combine::NweIndicatorCombine;
use crate::trading::strategy::nwe_strategy::NweSignalValues;
use crate::CandleItem;
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

// 定义最大容量常量
const MAX_CANDLE_ITEMS: usize = 100;

/// Nwe 策略指标值结构
#[derive(Debug, Clone)]
pub struct ArcNweIndicatorValues {
    pub timestamp: i64,
    pub inst_id: String,
    pub period: String,
    pub candle_item: VecDeque<CandleItem>,
    pub indicator_combines: NweIndicatorCombine,
}

impl Default for ArcNweIndicatorValues {
    fn default() -> Self {
        Self {
            timestamp: 0,
            inst_id: String::new(),
            period: String::new(),
            candle_item: VecDeque::new(),
            indicator_combines: NweIndicatorCombine::default(),
        }
    }
}

/// Nwe 指标值管理器
#[derive(Clone)]
pub struct NweIndicatorValuesManager {
    values: Arc<DashMap<String, ArcNweIndicatorValues>>,
    key_mutex: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl NweIndicatorValuesManager {
    pub fn new() -> Self {
        Self {
            values: Arc::new(DashMap::new()),
            key_mutex: Arc::new(DashMap::new()),
        }
    }

    /// 获取指标值快照
    pub async fn get_snapshot_last_n(
        &self,
        key: &str,
        n: usize,
    ) -> Option<(Vec<CandleItem>, NweIndicatorCombine, i64)> {
        self.values.get(key).map(|r| {
            let v = r.value();
            let len = v.candle_item.len();
            let take_n = n.min(len);
            let mut last_n: Vec<CandleItem> = Vec::with_capacity(take_n);
            for i in len.saturating_sub(take_n)..len {
                last_n.push(v.candle_item[i].clone());
            }
            (last_n, v.indicator_combines.clone(), v.timestamp)
        })
    }

    /// 设置指标值
    pub async fn set(&self, key: String, value: ArcNweIndicatorValues) -> Result<(), String> {
        let mut value_with_limited_history = value.clone();
        if value_with_limited_history.candle_item.len() > MAX_CANDLE_ITEMS {
            let excess = value_with_limited_history.candle_item.len() - MAX_CANDLE_ITEMS;
            for _ in 0..excess {
                value_with_limited_history.candle_item.pop_front();
            }
        }
        self.values.insert(key, value_with_limited_history);
        Ok(())
    }

    /// 原子更新K线和指标
    pub async fn update_both(
        &self,
        key: &str,
        candles: VecDeque<CandleItem>,
        indicators: NweIndicatorCombine,
        timestamp: i64,
    ) -> Result<(), String> {
        if !self.key_exists(key).await {
            return Err(format!("键 {} 不存在", key));
        }
        if let Some(mut entry) = self.values.get_mut(key) {
            let values = entry.value_mut();
            let mut new_candles = candles;
            if new_candles.len() > MAX_CANDLE_ITEMS {
                let excess = new_candles.len() - MAX_CANDLE_ITEMS;
                for _ in 0..excess {
                    new_candles.pop_front();
                }
            }
            values.candle_item = new_candles;
            values.indicator_combines = indicators;
            values.timestamp = timestamp;
            Ok(())
        } else {
            Err(format!("键 {} 不存在", key))
        }
    }

    /// 检查键是否存在
    pub async fn key_exists(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 获取键互斥锁
    pub async fn acquire_key_mutex(&self, key: &str) -> Arc<Mutex<()>> {
        self.key_mutex
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone()
    }
}

// 全局单例实例
pub static NWE_INDICATOR_MANAGER: OnceCell<NweIndicatorValuesManager> = OnceCell::new();

/// 获取全局 Nwe 管理器实例
pub fn get_nwe_indicator_manager() -> &'static NweIndicatorValuesManager {
    NWE_INDICATOR_MANAGER.get_or_init(|| NweIndicatorValuesManager::new())
}

/// 设置 Nwe 策略指标值
pub async fn set_nwe_strategy_indicator_values(
    inst_id: String,
    period: String,
    timestamp: i64,
    hash_key: String,
    candle_items: VecDeque<CandleItem>,
    values: NweIndicatorCombine,
) {
    let arc_nwe_indicator_values = ArcNweIndicatorValues {
        timestamp,
        inst_id,
        period,
        candle_item: candle_items,
        indicator_combines: values,
    };

    if let Err(e) = get_nwe_indicator_manager()
        .set(hash_key.clone(), arc_nwe_indicator_values)
        .await
    {
        tracing::error!("设置 Nwe 策略指标值失败: {}", e);
    } else {
        tracing::info!("Nwe 策略指标值已设置: {}", hash_key);
    }
}
```

#### 📝 修改文件: `src/trading/strategy/arc/indicator_values/mod.rs`

```rust
pub mod arc_vegas_indicator_values;
pub mod arc_nwe_indicator_values;  // 新增
pub mod ema_indicator_values;
```

---

### 🟢 **问题 3**: StrategyDataService 仅支持 Vegas

**文件**: `src/trading/services/strategy_data_service.rs`

#### ✅ 修改方案: 添加 Nwe 策略初始化支持

```rust
// 在文件顶部添加导入
use crate::trading::strategy::nwe_strategy::{NweStrategy, NweStrategyConfig};
use crate::trading::strategy::arc::indicator_values::arc_nwe_indicator_values;

// 修改 initialize_strategy_data 方法
impl StrategyDataService {
    pub async fn initialize_strategy_data(
        strategy: &StrategyConfig,
        inst_id: &str,
        time: &str,
    ) -> Result<StrategyDataSnapshot, StrategyDataError> {
        // ... 前面的代码保持不变 ...

        // 🔄 识别策略类型并初始化
        let strategy_type = detect_strategy_type(&strategy.strategy_config)?;
        
        match strategy_type {
            StrategyType::Vegas => {
                Self::initialize_vegas_data(
                    strategy, inst_id, time, 
                    candle_items, hash_key
                ).await
            }
            StrategyType::Nwe => {
                Self::initialize_nwe_data(
                    strategy, inst_id, time, 
                    candle_items, hash_key
                ).await
            }
            _ => Err(StrategyDataError::DataInitializationFailed {
                reason: format!("不支持的策略类型: {:?}", strategy_type),
            })
        }
    }

    /// 初始化 Nwe 策略数据（新增）
    async fn initialize_nwe_data(
        strategy: &StrategyConfig,
        inst_id: &str,
        time: &str,
        candle_items: VecDeque<CandleItem>,
        hash_key: String,
    ) -> Result<StrategyDataSnapshot, StrategyDataError> {
        // 1. 解析 Nwe 策略配置
        let nwe_config: NweStrategyConfig = 
            serde_json::from_str(&strategy.strategy_config)
                .map_err(|e| StrategyDataError::ValidationError {
                    field: format!("解析 NweStrategyConfig 失败: {}", e),
                })?;

        // 2. 创建 Nwe 策略实例
        let mut nwe_strategy = NweStrategy::new(nwe_config.clone());
        let mut indicator_combine = nwe_strategy.get_indicator_combine();

        // 3. 初始化指标值
        for item in candle_items.iter() {
            indicator_combine.next(item);
        }

        // 4. 获取最新时间戳
        let last_timestamp = candle_items
            .back()
            .map(|c| c.ts)
            .unwrap_or(0);

        // 5. 存储到 Nwe 缓存
        arc_nwe_indicator_values::set_nwe_strategy_indicator_values(
            inst_id.to_string(),
            time.to_string(),
            last_timestamp,
            hash_key.clone(),
            candle_items.clone(),
            indicator_combine.clone(),
        )
        .await;

        info!("Nwe 策略数据初始化成功: {}_{}", inst_id, time);

        // 6. 返回快照（注意：这里需要修改 StrategyDataSnapshot 结构以支持多策略）
        Ok(StrategyDataSnapshot {
            hash_key,
            candle_items,
            indicator_values: Default::default(), // 需要重构这个结构
            last_timestamp,
        })
    }
}

/// 检测策略类型
fn detect_strategy_type(strategy_config: &str) -> Result<StrategyType, StrategyDataError> {
    if serde_json::from_str::<VegasStrategy>(strategy_config).is_ok() {
        return Ok(StrategyType::Vegas);
    }
    if serde_json::from_str::<NweStrategyConfig>(strategy_config).is_ok() {
        return Ok(StrategyType::Nwe);
    }
    Err(StrategyDataError::ValidationError {
        field: "无法识别策略类型".to_string(),
    })
}
```

---

### 🟣 **问题 4**: NweIndicatorCombine 需要实现指标更新方法

**文件**: `src/trading/strategy/nwe_strategy/indicator_combine.rs`

#### ✅ 确认已有方法或新增:

```rust
impl NweIndicatorCombine {
    /// 推进所有指标并返回当前值
    pub fn next(&mut self, candle: &CandleItem) -> NweSignalValues {
        let rsi = if let Some(r) = &mut self.rsi_indicator {
            r.next(candle.c)
        } else {
            0.0
        };
        
        let volume_ratio = if let Some(v) = &mut self.volume_indicator {
            v.next(candle.v)
        } else {
            0.0
        };
        
        let (short_stop, long_stop, atr_value) = if let Some(a) = &mut self.atr_indicator {
            a.next(candle.h, candle.l, candle.c)
        } else {
            (0.0, 0.0, 0.0)
        };
        
        let (upper, lower) = if let Some(n) = &mut self.nwe_indicator {
            n.next(candle.c)
        } else {
            (0.0, 0.0)
        };
        
        NweSignalValues {
            rsi_value: rsi,
            volume_ratio,
            atr_value,
            atr_short_stop: short_stop,
            atr_long_stop: long_stop,
            nwe_upper: upper,
            nwe_lower: lower,
        }
    }
}
```

---

### 🔵 **问题 5**: SwapOrderService 支持策略类型

**文件**: `src/trading/services/order_service/swap_order_service.rs`

#### ✅ 确认方法签名:

```rust
impl SwapOrderService {
    pub async fn ready_to_order(
        &self,
        strategy_type: &StrategyType,  // ✅ 已支持策略类型参数
        inst_id: &str,
        period: &str,
        signal_result: &SignalResult,
        risk_config: &BasicRiskStrategyConfig,
        strategy_config_id: i64,
    ) -> Result<()> {
        // 实现内部应该已经支持不同策略类型
        // 确认是否需要针对 Nwe 策略做特殊处理
    }
}
```

---

## 四、完整实现示例：run_nwe_strategy

```rust
/// Nwe 策略执行函数
async fn run_nwe_strategy(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
    snap: Option<CandlesEntity>,
) -> Result<()> {
    const MAX_HISTORY_SIZE: usize = 10000;
    
    // 1. 获取策略类型和哈希键
    let strategy_type = StrategyType::Nwe.as_str().to_owned();
    let key = get_hash_key(inst_id, period, &strategy_type);
    let manager = arc_nwe_indicator_values::get_nwe_indicator_manager();
    
    // 2. 获取最新K线数据
    let new_candle_data = if let Some(snap) = snap {
        snap
    } else {
        CandleDomainService::new_default()
            .await
            .get_new_one_candle_fresh(inst_id, period, None)
            .await
            .map_err(|e| anyhow!("获取最新K线数据失败: {}", e))?
            .ok_or_else(|| anyhow!("获取的最新K线数据为空"))?
    };
    
    let new_candle_item = parse_candle_to_data_item(&new_candle_data);
    
    // 3. 获取互斥锁和缓存快照
    let key_mutex = manager.acquire_key_mutex(&key).await;
    let _guard = key_mutex.lock().await;
    
    let (mut last_candles_vec, mut old_indicator_combines, old_time) =
        match manager.get_snapshot_last_n(&key, MAX_HISTORY_SIZE).await {
            Some((v, indicators, ts)) => (v, indicators, ts),
            None => {
                return Err(anyhow!("没有找到对应的策略值: {}", key));
            }
        };
    
    // 4. 转换为 VecDeque
    let mut new_candle_items: VecDeque<CandleItem> = last_candles_vec.into_iter().collect();
    
    // 5. 检查是否为新K线
    if !check_new_time(
        old_time,
        new_candle_item.ts,
        period,
        new_candle_data.confirm == "1",
    ) {
        debug!("时间未更新或K线未确认,跳过本次策略执行");
        return Ok(());
    }
    
    // 6. 去重检查
    if !StrategyExecutionStateManager::try_mark_processing(&key, new_candle_item.ts) {
        return Ok(());
    }
    
    // 7. 添加新K线
    new_candle_items.push_back(new_candle_item.clone());
    if new_candle_items.len() > MAX_HISTORY_SIZE {
        new_candle_items.pop_front();
    }
    
    // 8. 更新指标值
    let new_indicator_values = old_indicator_combines.next(&new_candle_item);
    
    // 9. 更新缓存
    manager
        .update_both(
            &key,
            new_candle_items.clone(),
            old_indicator_combines.clone(),
            new_candle_item.ts,
        )
        .await
        .map_err(|e| anyhow!("更新指标值失败: {}", e))?;
    
    // 10. 转换为切片
    let candle_vec: Vec<CandleItem> = new_candle_items.into_iter().collect();
    
    // 11. 解析策略配置并生成信号
    let nwe_config: NweStrategyConfig =
        serde_json::from_str(&strategy.strategy_config)?;
    let mut nwe_strategy = NweStrategy::new(nwe_config);
    
    let signal_result = nwe_strategy.get_trade_signal(
        &candle_vec,
        &new_indicator_values,
    );
    
    info!(
        "Nwe 策略信号！inst_id:{:?} period:{:?}, should_buy:{}, should_sell:{}, ts:{}",
        inst_id,
        period,
        signal_result.should_buy,
        signal_result.should_sell,
        new_candle_item.ts
    );
    
    // 12. 如有信号则执行下单
    if signal_result.should_buy || signal_result.should_sell {
        // 记录信号日志
        save_signal_log(inst_id, period, &signal_result);
        
        // 解析风险配置
        let risk_config: BasicRiskStrategyConfig =
            serde_json::from_str(&strategy.risk_config)?;
        
        // 执行下单
        let res = SwapOrderService::new()
            .ready_to_order(
                &StrategyType::Nwe,  // ✅ 传递 Nwe 策略类型
                inst_id,
                period,
                &signal_result,
                &risk_config,
                strategy.strategy_config_id,
            )
            .await;
        
        match res {
            Ok(_) => {
                info!("Nwe 策略下单成功");
            }
            Err(e) => {
                error!("Nwe 策略下单失败: {}", e);
            }
        }
    } else {
        debug!("Nwe 策略: 无信号, ts:{}", new_candle_item.ts);
    }
    
    // 13. 清理执行状态
    StrategyExecutionStateManager::mark_completed(&key, new_candle_item.ts);
    
    Ok(())
}
```

---

## 五、测试验证清单

### 5.1 单元测试
- [ ] NweIndicatorValuesManager 缓存读写
- [ ] run_nwe_strategy 信号生成
- [ ] detect_strategy_type 类型识别

### 5.2 集成测试
- [ ] 启动 Nwe 策略成功
- [ ] K线更新触发策略执行
- [ ] 信号生成和订单执行
- [ ] 策略停止和重启

### 5.3 回归测试
- [ ] Vegas 策略仍然正常运行
- [ ] 多策略并行运行
- [ ] 策略切换和热更新

---

## 六、实施步骤建议

### Step 1: 创建基础设施 ✅
1. 创建 `arc_nwe_indicator_values.rs`
2. 修改 `mod.rs` 导出新模块

### Step 2: 重构 strategy_runner.rs ✅
1. 提取 `detect_strategy_type` 函数
2. 重构 `run_ready_to_order_with_manager` 添加分发逻辑
3. 提取 `run_vegas_strategy` 函数（保持原逻辑）
4. 实现 `run_nwe_strategy` 函数

### Step 3: 扩展 StrategyDataService ✅
1. 添加 `initialize_nwe_data` 方法
2. 修改 `initialize_strategy_data` 添加策略类型识别

### Step 4: 验证集成 ✅
1. 编写单元测试
2. 启动实盘测试
3. 监控日志和指标

---

## 七、关键注意事项

### ⚠️ 数据结构兼容性
- `StrategyDataSnapshot` 当前硬编码 Vegas 的 `IndicatorCombine`
- 建议重构为泛型或使用 trait object

### ⚠️ 指标更新频率
- 确保 `NweIndicatorCombine.next()` 与 `VegasIndicatorCombine` 一致
- 每根K线只计算一次

### ⚠️ 并发安全
- 使用 `DashMap` 和 `Mutex` 保证线程安全
- 每个策略键独立互斥

### ⚠️ 错误处理
- 策略执行失败不应影响其他策略
- 记录详细错误日志便于排查

---

## 八、性能优化建议

1. **指标缓存**: 限制历史K线数量（MAX_CANDLE_ITEMS=100）
2. **快照读取**: 使用 `get_snapshot_last_n` 避免全量克隆
3. **并发执行**: 不同策略/币对并行执行
4. **去重机制**: `StrategyExecutionStateManager` 防止重复处理

---

## 九、参考文件清单

| 文件路径 | 职责 | 修改优先级 |
|---------|------|----------|
| `src/trading/task/strategy_runner.rs` | 策略执行核心 | 🔴 高 |
| `src/trading/services/strategy_data_service.rs` | 数据初始化 | 🔴 高 |
| `src/trading/strategy/arc/indicator_values/arc_nwe_indicator_values.rs` | Nwe缓存（新建） | 🔴 高 |
| `src/trading/strategy/nwe_strategy/mod.rs` | Nwe策略逻辑 | 🟡 中 |
| `src/trading/strategy/nwe_strategy/indicator_combine.rs` | Nwe指标组合 | 🟡 中 |
| `src/trading/services/order_service/swap_order_service.rs` | 下单服务 | 🟢 低 |

---

**文档版本**: v1.0
**最后更新**: 2025-10-28
**作者**: AI Assistant

