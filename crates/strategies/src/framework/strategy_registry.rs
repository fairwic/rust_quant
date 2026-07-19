//! 策略注册中心
//!
//! 管理所有已注册的策略，提供策略的自动检测和获取功能
use super::strategy_trait::StrategyExecutor;
use crate::implementations::{
    BearShortStackStrategyExecutor, BscEventArbStrategyExecutor,
    BtcEthLiquidityScalperStrategyExecutor, KeltnerChannelScalperStrategyExecutor,
    MomentumBreakoutScalperStrategyExecutor, NweStrategyExecutor,
    RangeReversionScalperStrategyExecutor, SmartMoneyConceptsStrategyExecutor,
    VegasStrategyExecutor,
};
use crate::StrategyType;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};
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
    /// # 参数
    /// * `strategy` - 策略执行器实例
    /// # 示例
    /// ```rust,ignore
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
    /// 遍历所有已注册的策略，找到第一个能够处理该配置的策略
    /// # 参数
    /// * `strategy_config` - JSON 格式的策略配置
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
    /// 根据名称获取策略（大小写不敏感）
    /// # 参数
    /// * `name` - 策略名称（如 "Vegas", "vegas", "Nwe", "nwe"）
    /// # 返回
    /// * `Ok(Arc<dyn StrategyExecutor>)` - 找到策略
    /// * `Err` - 策略未注册
    pub fn get(&self, name: &str) -> Result<Arc<dyn StrategyExecutor>> {
        let strategies = self.strategies.read().expect("RwLock poisoned");
        // 先尝试精确匹配
        if let Some(strategy) = strategies.get(name) {
            return Ok(strategy.clone());
        }
        // 大小写不敏感查找
        let name_lower = name.to_lowercase();
        for (key, strategy) in strategies.iter() {
            if key.to_lowercase() == name_lower {
                return Ok(strategy.clone());
            }
        }
        let normalized_name = normalize_strategy_lookup_name(name);
        for (key, strategy) in strategies.iter() {
            if normalize_strategy_lookup_name(key) == normalized_name {
                return Ok(strategy.clone());
            }
        }
        Err(anyhow!("策略未注册: {}", name))
    }
    /// 列出所有已注册策略
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
    pub fn count(&self) -> usize {
        self.strategies.read().expect("RwLock poisoned").len()
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
/// Normalizes executor lookup names only; external `strategy_key` parsing remains version-strict.
fn normalize_strategy_lookup_name(name: &str) -> String {
    let normalized: String = name
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    let without_research = normalized
        .strip_suffix("research")
        .unwrap_or(normalized.as_str());
    strip_version_suffix(without_research).to_string()
}

/// Removes a trailing numeric executor version so `*_v1` keys can find Rust executor names.
/// This is not a compatibility alias: product keys are still validated in `StrategyType::from_str`
/// and each executor's `can_handle` implementation.
fn strip_version_suffix(name: &str) -> &str {
    let Some(version_start) = name.rfind('v') else {
        return name;
    };
    let suffix = &name[version_start + 1..];
    if version_start == 0 || suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return name;
    }
    &name[..version_start]
}
/// 初始化策略注册中心（空注册表，按需加载）
/// 策略将在首次使用时自动注册，而不是预先注册所有策略
fn initialize_registry() -> StrategyRegistry {
    let registry = StrategyRegistry::new();
    register_builtin_strategies(&registry);
    info!(
        "🎯 策略注册中心初始化完成，当前已注册 {} 个策略",
        registry.count()
    );
    registry
}
/// 全局策略注册中心（单例）
pub static STRATEGY_REGISTRY: Lazy<StrategyRegistry> = Lazy::new(initialize_registry);
pub fn get_strategy_registry() -> &'static StrategyRegistry {
    &STRATEGY_REGISTRY
}
/// 按需注册策略（线程安全，幂等操作）✨
/// 根据策略类型自动注册对应的执行器，如果已注册则跳过。
/// 这个函数是线程安全的，可以并发调用。
/// # 参数
/// * `strategy_type` - 策略类型枚举
/// # 示例
/// ```rust,ignore
/// register_strategy_on_demand(&StrategyType::Vegas);
/// register_strategy_on_demand(&StrategyType::Nwe);
/// ```
pub fn register_strategy_on_demand(strategy_type: &StrategyType) {
    let registry = get_strategy_registry();
    register_executor_for_type(registry, strategy_type);
}
/// 注册框架内置的策略执行器（可多次调用，幂等）
pub fn register_default_strategies() {
    let registry = get_strategy_registry();
    register_builtin_strategies(registry);
}
/// 注册 回测与策略研究 组件，使运行时可以按类型或名称找到对应实现。
fn register_builtin_strategies(registry: &StrategyRegistry) {
    const DEFAULT_TYPES: [StrategyType; 8] = [
        StrategyType::Vegas,
        StrategyType::VegasUniversal4h,
        StrategyType::Nwe,
        StrategyType::BscEventArb,
        StrategyType::BtcEthLiquidityScalper,
        StrategyType::BearShortStack,
        StrategyType::RangeReversionScalper,
        StrategyType::MomentumBreakoutScalper,
    ];
    for strategy_type in DEFAULT_TYPES.iter() {
        register_executor_for_type(registry, strategy_type);
    }
}
/// 注册 回测与策略研究 组件，使运行时可以按类型或名称找到对应实现。
fn register_executor_for_type(registry: &StrategyRegistry, strategy_type: &StrategyType) {
    let key = match strategy_type {
        StrategyType::Vegas => "Vegas",
        StrategyType::VegasUniversal4h => "VegasUniversal4h",
        StrategyType::Nwe => "Nwe",
        StrategyType::BscEventArb => "BscEventArb",
        StrategyType::BtcEthLiquidityScalper => "BtcEthLiquidityScalper",
        StrategyType::BearShortStack => "BearShortStack",
        StrategyType::RangeReversionScalper => "RangeReversionScalper",
        StrategyType::MomentumBreakoutScalper => "MomentumBreakoutScalper",
        StrategyType::SmartMoneyConceptsV1Research => "SmartMoneyConcepts",
        StrategyType::KeltnerChannelScalper1mV1Research => "KeltnerChannelScalper1m",
        _ => strategy_type.as_str(),
    };
    if registry.contains(key) {
        return;
    }
    match strategy_type {
        StrategyType::Vegas => {
            registry.register(Arc::new(VegasStrategyExecutor::new()));
            info!("✅ 注册策略: Vegas");
        }
        StrategyType::VegasUniversal4h => {
            registry.register(Arc::new(VegasStrategyExecutor::universal_4h()));
            info!("✅ 注册策略: VegasUniversal4h");
        }
        StrategyType::Nwe => {
            registry.register(Arc::new(NweStrategyExecutor::new()));
            info!("✅ 注册策略: Nwe");
        }
        StrategyType::BscEventArb => {
            registry.register(Arc::new(BscEventArbStrategyExecutor::new()));
            info!("✅ 注册策略: BscEventArb");
        }
        StrategyType::BtcEthLiquidityScalper => {
            registry.register(Arc::new(BtcEthLiquidityScalperStrategyExecutor::new()));
            info!("✅ 注册策略: BtcEthLiquidityScalper");
        }
        StrategyType::BearShortStack => {
            registry.register(Arc::new(BearShortStackStrategyExecutor::new()));
            info!("✅ 注册策略: BearShortStack");
        }
        StrategyType::RangeReversionScalper => {
            registry.register(Arc::new(RangeReversionScalperStrategyExecutor::new()));
            info!("✅ 注册策略: RangeReversionScalper");
        }
        StrategyType::MomentumBreakoutScalper => {
            registry.register(Arc::new(MomentumBreakoutScalperStrategyExecutor::new()));
            info!("✅ 注册策略: MomentumBreakoutScalper");
        }
        StrategyType::SmartMoneyConceptsV1Research => {
            registry.register(Arc::new(SmartMoneyConceptsStrategyExecutor::new()));
            info!("✅ 注册策略: SmartMoneyConcepts");
        }
        StrategyType::KeltnerChannelScalper1mV1Research => {
            registry.register(Arc::new(KeltnerChannelScalperStrategyExecutor::new()));
            info!("✅ 注册策略: KeltnerChannelScalper1m");
        }
        _ => {
            warn!("⚠️  策略类型 {:?} 暂未实现执行器，跳过注册", strategy_type);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::StrategyType;
    #[test]
    fn test_registry_singleton() {
        let registry1 = get_strategy_registry();
        let registry2 = get_strategy_registry();
        // 验证是同一个实例
        assert_eq!(registry1.count(), registry2.count());
    }
    #[test]
    fn test_on_demand_registration_is_idempotent() {
        let registry = get_strategy_registry();
        let initial_count = registry.count();
        register_strategy_on_demand(&StrategyType::Vegas);
        register_strategy_on_demand(&StrategyType::Vegas);
        assert!(registry.contains("Vegas"));
        assert_eq!(registry.count(), initial_count);
    }
    #[test]
    fn test_register_executor_for_type_new_registry() {
        let registry = StrategyRegistry::new();
        super::register_executor_for_type(&registry, &StrategyType::Vegas);
        assert!(registry.contains("Vegas"));
        super::register_executor_for_type(&registry, &StrategyType::VegasUniversal4h);
        assert!(registry.contains("VegasUniversal4h"));
        super::register_executor_for_type(&registry, &StrategyType::Nwe);
        assert!(registry.contains("Nwe"));
        assert_eq!(registry.count(), 3);
    }
    #[test]
    fn test_list_strategies_contains_defaults() {
        let registry = get_strategy_registry();
        let strategies = registry.list_strategies();
        assert!(strategies.iter().any(|s| s == "Vegas"));
        assert!(strategies.iter().any(|s| s == "Nwe"));
    }

    #[test]
    fn research_strategies_are_not_registered_by_default() {
        let registry = StrategyRegistry::new();
        super::register_builtin_strategies(&registry);

        assert!(!registry.contains("SmartMoneyConcepts"));
        assert!(!registry.contains("KeltnerChannelScalper1m"));
    }

    #[test]
    fn research_strategies_remain_available_by_explicit_registration() {
        let registry = StrategyRegistry::new();

        super::register_executor_for_type(&registry, &StrategyType::SmartMoneyConceptsV1Research);
        super::register_executor_for_type(
            &registry,
            &StrategyType::KeltnerChannelScalper1mV1Research,
        );

        assert!(registry.contains("SmartMoneyConcepts"));
        assert!(registry.contains("KeltnerChannelScalper1m"));
    }
}
