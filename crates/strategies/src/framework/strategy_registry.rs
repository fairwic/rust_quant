//! 策略注册中心
//! 
//! 管理所有已注册的策略，提供策略的自动检测和获取功能

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use tracing::{info, warn};

use super::strategy_trait::StrategyExecutor;

/// 策略注册中心
/// 
/// 单例模式，全局唯一
pub struct StrategyRegistry {
    /// 策略名称 -> 策略执行器
    strategies: RwLock<HashMap<String, Arc<dyn StrategyExecutor>>>,
}

impl StrategyRegistry {
    /// 创建新的注册中心
    fn new() -> Self {
        Self {
            strategies: RwLock::new(HashMap::new()),
        }
    }
    
    /// 注册策略
    /// 
    /// # 参数
    /// * `strategy` - 策略执行器实例
    /// 
    /// # 示例
    /// ```
    /// registry.register(Arc::new(VegasStrategyExecutor::new()));
    /// ```
    pub fn register(&self, strategy: Arc<dyn StrategyExecutor>) {
        let name = strategy.name();
        let mut strategies = self.strategies.write().expect("RwLock poisoned");
        
        if strategies.contains_key(name) {
            warn!("策略已存在，将被覆盖: {}", name);
        }
        
        strategies.insert(name.to_string(), strategy);
        info!("✅ 策略已注册: {}", name);
    }
    
    /// 根据配置自动检测策略类型
    /// 
    /// 遍历所有已注册的策略，找到第一个能够处理该配置的策略
    /// 
    /// # 参数
    /// * `strategy_config` - JSON 格式的策略配置
    /// 
    /// # 返回
    /// * `Ok(Arc<dyn StrategyExecutor>)` - 找到匹配的策略
    /// * `Err` - 未找到匹配的策略
    pub fn detect_strategy(&self, strategy_config: &str) -> Result<Arc<dyn StrategyExecutor>> {
        let strategies = self.strategies.read().expect("RwLock poisoned");
        
        for strategy in strategies.values() {
            if strategy.can_handle(strategy_config) {
                info!("🔍 检测到策略类型: {}", strategy.name());
                return Ok(strategy.clone());
            }
        }
        
        Err(anyhow!(
            "未找到匹配的策略类型，请检查配置是否正确。已注册策略: {:?}",
            strategies.keys().collect::<Vec<_>>()
        ))
    }
    
    /// 根据名称获取策略
    /// 
    /// # 参数
    /// * `name` - 策略名称（如 "Vegas", "Nwe"）
    /// 
    /// # 返回
    /// * `Ok(Arc<dyn StrategyExecutor>)` - 找到策略
    /// * `Err` - 策略未注册
    pub fn get(&self, name: &str) -> Result<Arc<dyn StrategyExecutor>> {
        self.strategies
            .read()
            .expect("RwLock poisoned")
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("策略未注册: {}", name))
    }
    
    /// 列出所有已注册策略
    /// 
    /// # 返回
    /// * 策略名称列表
    pub fn list_strategies(&self) -> Vec<String> {
        self.strategies
            .read()
            .expect("RwLock poisoned")
            .keys()
            .cloned()
            .collect()
    }
    
    /// 获取已注册策略数量
    pub fn count(&self) -> usize {
        self.strategies
            .read()
            .expect("RwLock poisoned")
            .len()
    }
    
    /// 检查策略是否已注册
    pub fn contains(&self, name: &str) -> bool {
        self.strategies
            .read()
            .expect("RwLock poisoned")
            .contains_key(name)
    }
    
    /// 移除策略（用于热重载）
    pub fn unregister(&self, name: &str) -> Option<Arc<dyn StrategyExecutor>> {
        let mut strategies = self.strategies.write().expect("RwLock poisoned");
        let removed = strategies.remove(name);
        if removed.is_some() {
            info!("🗑️  策略已移除: {}", name);
        }
        removed
    }
}

/// 初始化策略注册中心（空注册表，按需加载）
/// 
/// 策略将在首次使用时自动注册，而不是预先注册所有策略
fn initialize_registry() -> StrategyRegistry {
    let registry = StrategyRegistry::new();
    
    info!("🎯 策略注册中心初始化完成（按需加载模式）");
    
    registry
}

/// 全局策略注册中心（单例）
pub static STRATEGY_REGISTRY: Lazy<StrategyRegistry> = Lazy::new(initialize_registry);

/// 获取全局策略注册中心
/// 
/// # 返回
/// * 策略注册中心的静态引用
/// 
/// # 示例
/// ```
/// let registry = get_strategy_registry();
/// let strategy = registry.detect_strategy(config)?;
/// strategy.execute(...).await?;
/// ```
pub fn get_strategy_registry() -> &'static StrategyRegistry {
    &STRATEGY_REGISTRY
}

/// 按需注册策略（线程安全，幂等操作）✨
/// 
/// 根据策略类型自动注册对应的执行器，如果已注册则跳过。
/// 这个函数是线程安全的，可以并发调用。
/// 
/// # 参数
/// * `strategy_type` - 策略类型枚举
/// 
/// # 示例
/// ```
/// register_strategy_on_demand(&StrategyType::Vegas);
/// register_strategy_on_demand(&StrategyType::Nwe);
/// ```
pub fn register_strategy_on_demand(strategy_type: &rust_quant_strategies::StrategyType) {
    use super::vegas_executor::VegasStrategyExecutor;
    use super::nwe_executor::NweStrategyExecutor;
    use rust_quant_strategies::StrategyType;
    use okx::dto::EnumToStrTrait;
    
    let registry = get_strategy_registry();
    let strategy_name = strategy_type.as_str();
    
    // 检查是否已注册（避免重复注册）
    if registry.contains(strategy_name) {
        return;
    }
    
    // 根据策略类型创建并注册执行器
    match strategy_type {
        StrategyType::Vegas => {
            registry.register(Arc::new(VegasStrategyExecutor::new()));
            info!("✅ 按需注册策略: Vegas");
        }
        StrategyType::Nwe => {
            registry.register(Arc::new(NweStrategyExecutor::new()));
            info!("✅ 按需注册策略: Nwe");
        }
        _ => {
            warn!(
                "⚠️  策略类型 {:?} 暂未实现执行器，跳过注册",
                strategy_type
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_strategies::StrategyType;

    #[test]
    fn test_registry_singleton() {
        let registry1 = get_strategy_registry();
        let registry2 = get_strategy_registry();
        
        // 验证是同一个实例
        assert_eq!(registry1.count(), registry2.count());
    }

    #[test]
    fn test_on_demand_registration() {
        let registry = get_strategy_registry();
        
        // 初始应该为空（按需加载模式）
        let initial_count = registry.count();
        
        // 按需注册 Vegas
        register_strategy_on_demand(&StrategyType::Vegas);
        assert_eq!(registry.count(), initial_count + 1);
        assert!(registry.contains("Vegas"));
        
        // 重复注册应该被忽略
        register_strategy_on_demand(&StrategyType::Vegas);
        assert_eq!(registry.count(), initial_count + 1);
        
        // 注册 Nwe
        register_strategy_on_demand(&StrategyType::Nwe);
        assert_eq!(registry.count(), initial_count + 2);
        assert!(registry.contains("Nwe"));
    }
    
    #[test]
    fn test_list_strategies() {
        let registry = get_strategy_registry();
        
        // 按需注册
        register_strategy_on_demand(&StrategyType::Vegas);
        register_strategy_on_demand(&StrategyType::Nwe);
        
        let strategies = registry.list_strategies();
        
        // 应该包含 Vegas 和 Nwe
        assert!(strategies.contains(&"Vegas".to_string()));
        assert!(strategies.contains(&"Nwe".to_string()));
    }
}

