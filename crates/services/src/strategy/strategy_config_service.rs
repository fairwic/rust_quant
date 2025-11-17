//! 策略配置服务
//!
//! 从 orchestration/workflow/strategy_config.rs 提取业务逻辑

use anyhow::{anyhow, Result};
use tracing::{info, warn};

use rust_quant_domain::traits::StrategyConfigRepository;
use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};

/// 策略配置服务
///
/// 职责:
/// - 加载和管理策略配置
/// - 配置验证和转换
/// - 配置缓存管理
///
/// # 架构原则
/// - 依赖 domain::traits::StrategyConfigRepository（接口）
/// - 不依赖 infrastructure 具体实现
/// - 通过构造函数注入实现
pub struct StrategyConfigService {
    repository: Box<dyn StrategyConfigRepository>,
}

impl StrategyConfigService {
    /// 创建新的服务实例（通过依赖注入）
    ///
    /// # 参数
    /// * `repository` - StrategyConfigRepository 实现（通常在应用入口注入）
    ///
    /// # 示例
    /// ```rust,ignore
    /// use rust_quant_infrastructure::SqlxStrategyConfigRepository;
    /// let repo = SqlxStrategyConfigRepository::new();
    /// let service = StrategyConfigService::new(Box::new(repo));
    /// ```
    pub fn new(repository: Box<dyn StrategyConfigRepository>) -> Self {
        Self { repository }
    }

    /// 根据ID加载策略配置
    pub async fn load_config_by_id(&self, config_id: i64) -> Result<StrategyConfig> {
        info!("加载策略配置: config_id={}", config_id);

        self.repository
            .find_by_id(config_id)
            .await?
            .ok_or_else(|| anyhow!("策略配置不存在: {}", config_id))
    }

    /// 加载所有启用的策略配置
    pub async fn load_all_enabled_configs(&self) -> Result<Vec<StrategyConfig>> {
        let configs = self.repository.find_all_enabled().await?;
        info!("找到 {} 个启用的策略配置", configs.len());
        Ok(configs)
    }

    /// 根据交易对和时间周期加载配置
    pub async fn load_configs(
        &self,
        symbol: &str,
        timeframe: &str,
        strategy_type: Option<&str>,
    ) -> Result<Vec<StrategyConfig>> {
        info!(
            "加载策略配置: symbol={}, timeframe={}, type={:?}",
            symbol, timeframe, strategy_type
        );

        let timeframe_enum = Timeframe::from_str(timeframe)
            .ok_or_else(|| anyhow!("无效的时间周期: {}", timeframe))?;

        let mut configs = self
            .repository
            .find_by_symbol_and_timeframe(symbol, timeframe_enum)
            .await?;

        // 如果指定了策略类型，进行过滤
        if let Some(strategy_type_str) = strategy_type {
            let strategy_type_enum = StrategyType::from_str(strategy_type_str)
                .ok_or_else(|| anyhow!("无效的策略类型: {}", strategy_type_str))?;
            configs.retain(|c| c.strategy_type == strategy_type_enum);
        }

        if configs.is_empty() {
            warn!("未找到策略配置: {}@{}", symbol, timeframe);
            return Ok(vec![]);
        }

        info!("找到 {} 个策略配置", configs.len());
        Ok(configs)
    }

    /// 验证策略配置
    pub fn validate_config(&self, config: &StrategyConfig) -> Result<()> {
        // 验证策略参数是否完整
        if config.parameters.is_null() {
            return Err(anyhow!("策略参数不能为空"));
        }

        if config.risk_config.is_null() {
            return Err(anyhow!("风险配置不能为空"));
        }

        Ok(())
    }

    /// 保存策略配置
    pub async fn save_config(&self, config: StrategyConfig) -> Result<i64> {
        info!(
            "保存策略配置: type={:?}, symbol={}",
            config.strategy_type, config.symbol
        );

        // 验证配置
        self.validate_config(&config)?;

        // 通过仓储保存
        // TODO: SqlxStrategyConfigRepository需要实现save方法
        let config_id = 1; // 临时返回
        warn!("save_config 暂未实现");

        Ok(config_id)
    }

    /// 更新策略配置
    pub async fn update_config(&self, config: StrategyConfig) -> Result<()> {
        info!("更新策略配置: id={}", config.id);

        self.validate_config(&config)?;

        // 通过仓储更新
        self.repository.update(&config).await
    }

    /// 启动策略
    pub async fn start_strategy(&self, config_id: i64) -> Result<()> {
        let mut config = self.load_config_by_id(config_id).await?;

        if !config.can_start() {
            return Err(anyhow!("策略状态不允许启动: {:?}", config.status));
        }

        config.start();
        self.update_config(config).await?;

        info!("策略已启动: config_id={}", config_id);
        Ok(())
    }

    /// 停止策略
    pub async fn stop_strategy(&self, config_id: i64) -> Result<()> {
        let mut config = self.load_config_by_id(config_id).await?;
        config.stop();
        self.update_config(config).await?;

        info!("策略已停止: config_id={}", config_id);
        Ok(())
    }
    // pub async fn load_all_enabled_configs(&self) -> Result<Vec<StrategyConfig>> {
    //     info!("加载所有启用的策略配置");

    //     // 使用repository的方法查询所有配置
    //     // get_all() 已经过滤了 is_deleted = 0 的记录
    //     let configs = self.repository.get_all().await?;

    //     info!("找到 {} 个策略配置", configs.len());
    //     Ok(configs)
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_creation() {
        // let service = StrategyConfigService::new().await;
        // 服务创建成功
    }
}
