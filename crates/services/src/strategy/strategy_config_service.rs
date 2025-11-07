//! 策略配置服务
//! 
//! 从 orchestration/workflow/strategy_config.rs 提取业务逻辑

use anyhow::{anyhow, Result};
use tracing::{info, warn};

use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};
use rust_quant_infrastructure::{StrategyConfigEntity, StrategyConfigEntityModel};

/// 策略配置服务
/// 
/// 职责: 
/// - 加载和管理策略配置
/// - 配置验证和转换
/// - 配置缓存管理
pub struct StrategyConfigService {
    repository: StrategyConfigEntityModel,
}

impl StrategyConfigService {
    /// 创建新的服务实例
    pub async fn new() -> Self {
        Self {
            repository: StrategyConfigEntityModel::new().await,
        }
    }
    
    /// 根据ID加载策略配置
    pub async fn load_config_by_id(&self, config_id: i64) -> Result<StrategyConfig> {
        info!("加载策略配置: config_id={}", config_id);
        
        let entity = self.repository
            .get_config_by_id(config_id)
            .await?
            .ok_or_else(|| anyhow!("策略配置不存在: {}", config_id))?;
        
        // 转换为领域模型
        entity.to_domain()
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
        
        let entities = self.repository
            .get_config(strategy_type, symbol, timeframe)
            .await?;
        
        if entities.is_empty() {
            warn!("未找到策略配置: {}@{}", symbol, timeframe);
            return Ok(vec![]);
        }
        
        info!("找到 {} 个策略配置", entities.len());
        
        // 批量转换为领域模型
        let mut configs = Vec::with_capacity(entities.len());
        for entity in entities {
            match entity.to_domain() {
                Ok(config) => configs.push(config),
                Err(e) => {
                    warn!("配置转换失败: {}, id={}", e, entity.id);
                }
            }
        }
        
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
        info!("保存策略配置: type={:?}, symbol={}", config.strategy_type, config.symbol);
        
        // 验证配置
        self.validate_config(&config)?;
        
        // 通过仓储保存
        let config_id = self.repository
            .repository
            .save(&config)
            .await?;
        
        Ok(config_id)
    }
    
    /// 更新策略配置
    pub async fn update_config(&self, config: StrategyConfig) -> Result<()> {
        info!("更新策略配置: id={}", config.id);
        
        self.validate_config(&config)?;
        
        self.repository
            .repository
            .update(&config)
            .await?;
        
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_service_creation() {
        let service = StrategyConfigService::new().await;
        // 服务创建成功
    }
}

