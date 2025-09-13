use anyhow::{anyhow, Result};
use dashmap::DashMap;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::trading::indicator::vegas_indicator::VegasStrategy;
use crate::trading::model::strategy::strategy_config::{
    StrategyConfigEntity, StrategyConfigEntityModel,
};
use crate::trading::services::scheduler_service::SchedulerService;
use crate::trading::services::strategy_data_service::StrategyDataService;
use crate::trading::services::strategy_system_error::{
    StrategySystemError, StrategyConfigError, BusinessLogicError, ErrorHandler, ErrorSeverity
};
use crate::trading::services::strategy_metrics::{get_strategy_metrics, StrategyMetrics};
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use crate::trading::strategy::StrategyType;
use crate::SCHEDULER;
use okx::dto::EnumToStrTrait;

/// 策略管理器错误类型
#[derive(Error, Debug)]
pub enum StrategyManagerError {
    #[error("策略配置不存在: {config_id}")]
    ConfigNotFound { config_id: i64 },

    #[error("策略已在运行: {strategy_key}")]
    StrategyAlreadyRunning { strategy_key: String },

    #[error("策略未运行: {strategy_key}")]
    StrategyNotRunning { strategy_key: String },

    #[error("策略未处于暂停状态: {strategy_key}")]
    StrategyNotPaused { strategy_key: String },

    #[error("配置解析失败: {field}")]
    ConfigParseError { field: String },

    #[error("数据库操作失败: {operation}")]
    DatabaseError { operation: String },

    #[error("调度器未初始化")]
    SchedulerNotInitialized,

    #[error("策略停止失败: {reason}")]
    StrategyStopFailed { reason: String },

    #[error("配置序列化失败: {reason}")]
    ConfigSerializationError { reason: String },
}

/// 常量定义
const CONFIG_LOAD_TIMEOUT_SECS: u64 = 30;
const STRATEGY_KEY_SEPARATOR: &str = "_";
const DEFAULT_CONFIG_VERSION: u64 = 1;
const SCHEDULER_OPERATION_TIMEOUT_SECS: u64 = 5;

/// 策略运行状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyStatus {
    /// 运行中
    Running,
    /// 已停止
    Stopped,
    /// 暂停中
    Paused,
    /// 更新中
    Updating,
    /// 错误状态
    Error(String),
}

/// 策略运行时信息
#[derive(Debug, Clone)]
pub struct StrategyRuntimeInfo {
    /// 策略配置ID
    pub strategy_config_id: i64,
    /// 产品ID
    pub inst_id: String,
    /// 时间周期
    pub period: String,
    /// 策略类型
    pub strategy_type: String,
    /// 运行状态
    pub status: StrategyStatus,
    /// 任务UUID
    pub job_uuid: Option<Uuid>,
    /// 启动时间
    pub start_time: i64,
    /// 最后更新时间
    pub last_update_time: i64,
    /// 当前配置对象（支持热更新）
    pub current_config: Arc<RwLock<StrategyConfig>>,
    /// 配置版本号（用于变更追踪）
    pub config_version: u64,
}

// UUID 序列化辅助模块
mod uuid_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use uuid::Uuid;

    pub fn serialize<S>(uuid: &Option<Uuid>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match uuid {
            Some(u) => u.to_string().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => Uuid::parse_str(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// 策略配置快照（用于序列化）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StrategyConfigSnapshot {
    /// Vegas策略配置
    pub strategy_config: String,
    /// 风险配置
    pub risk_config: String,
}

impl StrategyRuntimeInfo {
    /// 创建新的运行时信息
    pub fn new(
        strategy_config_id: i64,
        inst_id: String,
        period: String,
        strategy_type: String,
        config: Arc<StrategyConfig>,
    ) -> Self {
        Self::new_with_job_uuid(strategy_config_id, inst_id, period, strategy_type, Arc::new(RwLock::new((*config).clone())), None)
    }

    /// 创建新的运行时信息（指定job_uuid）
    pub fn new_with_job_uuid(
        strategy_config_id: i64,
        inst_id: String,
        period: String,
        strategy_type: String,
        config: Arc<RwLock<StrategyConfig>>,
        job_uuid: Option<Uuid>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            strategy_config_id,
            inst_id,
            period,
            strategy_type,
            status: StrategyStatus::Running,
            job_uuid,
            start_time: now,
            last_update_time: now,
            current_config: config,
            config_version: DEFAULT_CONFIG_VERSION,
        }
    }

    /// 获取配置的JSON快照（按需序列化）
    pub async fn get_config_snapshot(&self) -> Result<StrategyConfigSnapshot> {
        let config_guard = self.current_config.read().await;
        let strategy_json = serde_json::to_string(&config_guard.strategy_config)
            .map_err(|e| anyhow!("序列化策略配置失败: {}", e))?;

        let risk_json = serde_json::to_string(&config_guard.risk_config)
            .map_err(|e| anyhow!("序列化风险配置失败: {}", e))?;

        Ok(StrategyConfigSnapshot {
            strategy_config: strategy_json,
            risk_config: risk_json,
        })
    }

    /// 获取当前的配置副本（用于只读访问）
    pub async fn get_current_config(&self) -> StrategyConfig {
        let config_guard = self.current_config.read().await;
        config_guard.clone()
    }

    /// 热更新配置（不重启策略）
    pub async fn hot_update_config(&self, new_config: StrategyConfig) -> Result<()> {
        let mut config_guard = self.current_config.write().await;
        *config_guard = new_config;
        Ok(())
    }

    /// 增加配置版本号
    pub fn increment_version(&mut self) {
        self.config_version = self.config_version.saturating_add(1);
        self.last_update_time = chrono::Utc::now().timestamp_millis();
    }

    /// 检查策略是否可以更新
    pub fn can_update(&self) -> bool {
        matches!(self.status, StrategyStatus::Running | StrategyStatus::Paused)
    }

    /// 获取用于序列化的包装器
    pub async fn to_serializable(&self) -> SerializableStrategyRuntimeInfo {
        SerializableStrategyRuntimeInfo {
            strategy_config_id: self.strategy_config_id,
            inst_id: self.inst_id.clone(),
            period: self.period.clone(),
            strategy_type: self.strategy_type.clone(),
            status: self.status.clone(),
            job_uuid: self.job_uuid,
            start_time: self.start_time,
            last_update_time: self.last_update_time,
            current_config: self.get_config_snapshot().await.unwrap_or_default(),
            config_version: self.config_version,
        }
    }
}

/// 可序列化的策略运行时信息（用于API返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableStrategyRuntimeInfo {
    /// 策略配置ID
    pub strategy_config_id: i64,
    /// 产品ID
    pub inst_id: String,
    /// 时间周期
    pub period: String,
    /// 策略类型
    pub strategy_type: String,
    /// 运行状态
    pub status: StrategyStatus,
    /// 任务UUID (序列化为字符串)
    #[serde(with = "uuid_serde")]
    pub job_uuid: Option<Uuid>,
    /// 启动时间
    pub start_time: i64,
    /// 最后更新时间
    pub last_update_time: i64,
    /// 当前配置快照
    pub current_config: StrategyConfigSnapshot,
    /// 配置版本号
    pub config_version: u64,
}

/// 策略管理器 - 单例模式（移除 StrategyOrder 依赖）
#[derive(Debug, Clone)]
pub struct StrategyManager {
    /// 运行中的策略信息: strategy_key -> RuntimeInfo
    running_strategies: Arc<DashMap<String, StrategyRuntimeInfo>>,
    /// 配置版本计数器（用于生成唯一版本号）
    config_version_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl StrategyManager {
    /// 创建策略管理器实例
    pub fn new() -> Self {
        Self {
            running_strategies: Arc::new(DashMap::new()),
            config_version_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// 创建共享配置对象（统一配置管理）
    fn create_shared_config(&self, strategy_config: &StrategyConfig) -> Arc<RwLock<StrategyConfig>> {
        Arc::new(RwLock::new(strategy_config.clone()))
    }

    /// 生成下一个配置版本号
    fn next_config_version(&self) -> u64 {
        self.config_version_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// 注册策略运行时信息
    pub async fn register_strategy_runtime(
        &self,
        strategy_config_id: i64,
        inst_id: String,
        period: String,
        strategy_type: String,
        config: StrategyConfig,
        job_uuid: Option<Uuid>,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(&inst_id, &period, &strategy_type);

        // 检查是否已存在
        if self.running_strategies.contains_key(&strategy_key) {
            return Err(anyhow!("策略已存在: {}", strategy_key));
        }

        // 创建运行时信息
        let runtime_info = StrategyRuntimeInfo::new_with_job_uuid(
            strategy_config_id,
            inst_id,
            period,
            strategy_type,
            Arc::new(RwLock::new(config)),
            job_uuid,
        );

        // 存储运行时信息
        self.running_strategies.insert(strategy_key.clone(), runtime_info);

        debug!("策略运行时信息已注册: {}", strategy_key);
        Ok(())
    }

    /// 验证策略参数
    fn validate_strategy_params(&self, inst_id: &str, period: &str) -> Result<()> {
        if inst_id.trim().is_empty() {
            return Err(anyhow!("产品ID不能为空"));
        }
        if period.trim().is_empty() {
            return Err(anyhow!("时间周期不能为空"));
        }
        Ok(())
    }

    /// 获取运行策略的数量
    pub fn running_strategy_count(&self) -> usize {
        self.running_strategies.len()
    }

    /// 检查调度器是否已初始化
    async fn ensure_scheduler_initialized(&self) -> Result<()> {
        let scheduler_lock = crate::SCHEDULER.lock().await;
        if scheduler_lock.is_none() {
            return Err(StrategyManagerError::SchedulerNotInitialized.into());
        }
        Ok(())
    }

    /// 构建策略唯一标识
    fn build_strategy_key(inst_id: &str, period: &str, strategy_type: &str) -> String {
        format!("{}{}{}{}{}", strategy_type, STRATEGY_KEY_SEPARATOR, inst_id, STRATEGY_KEY_SEPARATOR, period)
    }

    /// 检查策略是否正在运行
    pub fn is_strategy_running(&self, strategy_key: &str) -> bool {
        self.running_strategies.contains_key(strategy_key)
    }

    /// 获取运行中的策略信息
    fn get_running_strategy(&self, strategy_key: &str) -> Option<StrategyRuntimeInfo> {
        self.running_strategies.get(strategy_key).map(|v| v.clone())
    }

    /// 异步加载和解析策略配置
    async fn load_strategy_config(&self, strategy_config_id: i64) -> Result<(StrategyConfigEntity, Arc<StrategyConfig>)> {
        debug!("加载策略配置: config_id={}", strategy_config_id);

        // 设置超时
        let config_result = tokio::time::timeout(
            std::time::Duration::from_secs(CONFIG_LOAD_TIMEOUT_SECS),
            async {
                let config_model = StrategyConfigEntityModel::new().await;
                config_model.get_config_by_id(strategy_config_id).await
            }
        ).await
        .map_err(|_| anyhow!("配置加载超时"))?
        .map_err(|e| StrategyManagerError::DatabaseError {
            operation: format!("获取策略配置失败: {}", e)
        })?;

        if config_result.is_empty() {
            return Err(StrategyManagerError::ConfigNotFound { config_id: strategy_config_id }.into());
        }

        let config_entity = &config_result[0];

        // 解析策略配置
        let vegas_strategy: VegasStrategy = serde_json::from_str(&config_entity.value)
            .map_err(|e| StrategyManagerError::ConfigParseError {
                field: format!("VegasStrategy: {}", e)
            })?;

        let risk_config: BasicRiskStrategyConfig = serde_json::from_str(&config_entity.risk_config)
            .map_err(|e| StrategyManagerError::ConfigParseError {
                field: format!("BasicRiskStrategyConfig: {}", e)
            })?;

        let strategy_config = Arc::new(StrategyConfig {
            strategy_config_id: strategy_config_id.try_into().unwrap(),
            strategy_config: serde_json::to_string(&vegas_strategy).unwrap(),
            risk_config: serde_json::to_string(&risk_config).unwrap(),
        });

        Ok((config_entity.clone(), strategy_config))
    }

    /// 启动策略
    pub async fn start_strategy(
        &self,
        strategy_config_id: i64,
        inst_id: String,
        period: String,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        
        // 参数验证
        if strategy_config_id <= 0 {
            return Err(anyhow!("策略配置ID必须大于0"));
        }
        self.validate_strategy_params(&inst_id, &period)?;

        info!(
            "启动策略: config_id={}, inst_id={}, period={}",
            strategy_config_id, inst_id, period
        );

        // 1. 加载策略配置
        let (config_entity, strategy_config) = self.load_strategy_config(strategy_config_id).await?;

        // 2. 检查策略是否已在运行
        let strategy_key = Self::build_strategy_key(&inst_id, &period, &config_entity.strategy_type);
        if self.is_strategy_running(&strategy_key) {
            let error = StrategySystemError::Business(BusinessLogicError::StrategyAlreadyRunning {
                strategy_key: strategy_key.clone(),
            });
            ErrorHandler::handle_error(&error, &format!("启动策略: {}", strategy_key));
            return Err(anyhow!("策略已在运行: {}", strategy_key));
        }

        // 4. 根据策略类型执行具体的启动逻辑
        match config_entity.strategy_type.as_str() {
            "Vegas" => {
                let strategy_config_for_init = StrategyConfig {
                    strategy_config_id: strategy_config.strategy_config_id,
                    strategy_config: strategy_config.strategy_config.clone(),
                    risk_config: strategy_config.risk_config.clone(),
                };

                // 初始化策略数据（使用新的数据服务）
                let _data_snapshot = StrategyDataService::initialize_strategy_data(
                    &strategy_config_for_init,
                    &inst_id,
                    &period,
                ).await
                .map_err(|e| {
                    let error = StrategySystemError::Data(e);
                    ErrorHandler::handle_error(&error, &format!("启动策略-数据初始化: {}", strategy_key));
                    anyhow!("策略数据初始化失败: {}", error)
                })?;

                // 创建共享配置对象
                let shared_config = self.create_shared_config(&strategy_config_for_init);

                // 创建定时任务（使用新的调度器服务）
                let scheduled_job = SchedulerService::create_scheduled_job(
                    inst_id.clone(),
                    period.clone(),
                    config_entity.strategy_type.clone(),
                    shared_config.clone(),
                ).map_err(|e| {
                    let error = StrategySystemError::Scheduler(e);
                    ErrorHandler::handle_error(&error, &format!("启动策略-创建任务: {}", strategy_key));
                    anyhow!("创建定时任务失败: {}", error)
                })?;

                // 注册任务到调度器（使用新的调度器服务）
                let job_id = SchedulerService::register_job(scheduled_job)
                    .await
                    .map_err(|e| {
                        let error = StrategySystemError::Scheduler(e);
                        ErrorHandler::handle_error(&error, &format!("启动策略-注册任务: {}", strategy_key));
                        anyhow!("注册任务失败: {}", error)
                    })?;

                // 5. 记录运行信息（使用共享配置）
                let runtime_info = StrategyRuntimeInfo::new_with_job_uuid(
                    strategy_config_id,
                    inst_id.clone(),
                    period.clone(),
                    config_entity.strategy_type.clone(),
                    shared_config,
                    Some(job_id),
                );

                self.running_strategies.insert(strategy_key.clone(), runtime_info);
            }
            _ => {
                let error = StrategySystemError::Business(BusinessLogicError::UnsupportedStrategyType {
                    strategy_type: config_entity.strategy_type.clone(),
                });
                ErrorHandler::handle_error(&error, &format!("启动策略: {}", strategy_key));
                return Err(anyhow!("不支持的策略类型: {}", config_entity.strategy_type));
            }
        }

        // 记录启动性能指标
        let metrics = get_strategy_metrics();
        metrics.record_strategy_start_time(&strategy_key, start_time.elapsed()).await;

        info!("策略启动成功: {}", strategy_key);
        Ok(())
    }

    /// 停止策略
    pub async fn stop_strategy(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Result<()> {
        let stop_start_time = std::time::Instant::now();
        
        // 参数验证
        self.validate_strategy_params(inst_id, period)?;

        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        info!("停止策略: {}", strategy_key);

        // 1. 原子性地获取并移除策略信息，避免状态不一致
        let runtime_info = self
            .running_strategies
            .remove(&strategy_key)
            .ok_or_else(|| {
                let error = StrategySystemError::Business(BusinessLogicError::StrategyNotRunning {
                    strategy_key: strategy_key.clone(),
                });
                ErrorHandler::handle_error(&error, &format!("停止策略: {}", strategy_key));
                StrategyManagerError::StrategyNotRunning {
                    strategy_key: strategy_key.clone()
                }
            })?;

        let job_id = runtime_info.1.job_uuid;

        // 2. 异步移除调度器任务（使用新的调度器服务）
        if let Some(job_id) = job_id {
            if let Err(e) = SchedulerService::remove_job_safe(job_id).await {
                let error = StrategySystemError::Scheduler(e);
                ErrorHandler::handle_error(&error, &format!("停止策略-移除任务: {}", strategy_key));
                // 不返回错误，因为策略状态已经移除
            }
        }

        // 记录停止性能指标
        let metrics = get_strategy_metrics();
        metrics.record_strategy_stop_time(&strategy_key, stop_start_time.elapsed()).await;

        info!("策略已停止: {}", strategy_key);
        Ok(())
    }


    /// 更新运行中的策略配置
    pub async fn update_strategy_config(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
        new_config: UpdateStrategyConfigRequest,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        info!("更新策略配置: {}", strategy_key);

        // 1. 检查策略是否在运行
        let runtime_info = self
            .running_strategies
            .get(&strategy_key)
            .ok_or_else(|| anyhow!("策略未运行: {}", strategy_key))?;

        let strategy_config_id = runtime_info.strategy_config_id;
        drop(runtime_info); // 释放锁

        // 2. 标记为更新中
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Updating;
        }

        // 3. 停止当前策略
        if let Err(e) = self.stop_strategy(inst_id, period, strategy_type).await {
            // 恢复运行状态
            if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
                entry.status = StrategyStatus::Running;
            }
            return Err(anyhow!("停止策略失败: {}", e));
        }

        // 4. 更新数据库配置
        if let Some(ref strategy_config) = new_config.strategy_config {
            // TODO: 更新数据库中的策略配置
            let config_model = StrategyConfigEntityModel::new().await;
            config_model
                .update_strategy_config(strategy_config_id, strategy_config)
                .await?;
        }

        if let Some(ref risk_config) = new_config.risk_config {
            // TODO: 更新数据库中的风险配置
            let config_model = StrategyConfigEntityModel::new().await;
            config_model
                .update_risk_config(strategy_config_id, risk_config)
                .await?;
        }

        // 5. 重新加载配置并启动策略
        let config_model = StrategyConfigEntityModel::new().await;
        let configs = config_model.get_config_by_id(strategy_config_id).await?;

        if configs.is_empty() {
            return Err(anyhow!("策略配置不存在"));
        }

        let config_entity = &configs[0];
        let vegas_strategy: VegasStrategy = serde_json::from_str(&config_entity.value)?;
        let risk_config: BasicRiskStrategyConfig =
            serde_json::from_str(&config_entity.risk_config)?;

        let strategy_config = StrategyConfig {
            strategy_config_id: strategy_config_id.try_into().unwrap(),
            strategy_config: serde_json::to_string(&vegas_strategy).unwrap(),
            risk_config: serde_json::to_string(&risk_config).unwrap(),
        };

        // 6. 重新启动策略
        self.start_strategy(strategy_config_id, inst_id.to_string(), period.to_string())
            .await?;

        // 7. 更新运行信息（使用统一配置管理）
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Running;
            entry.hot_update_config(strategy_config.clone()).await.ok();
            entry.increment_version(); // 增加版本号
        }

        info!("策略配置更新成功: {}", strategy_key);
        Ok(())
    }

    /// 暂停策略
    pub async fn pause_strategy(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);

        // 1. 获取运行时信息并移除调度器任务
        let runtime_info = self
            .running_strategies
            .get(&strategy_key)
            .ok_or_else(|| anyhow!("策略未运行: {}", strategy_key))?;

        if let Some(job_id) = runtime_info.job_uuid {
            if let Err(e) = SchedulerService::remove_job_safe(job_id).await {
                let error = StrategySystemError::Scheduler(e);
                ErrorHandler::handle_error(&error, &format!("暂停策略-移除任务: {}", strategy_key));
            }
        }

        // 2. 更新状态为暂停
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Paused;
            entry.last_update_time = chrono::Utc::now().timestamp_millis();
        }

        info!("策略已暂停: {}", strategy_key);
        Ok(())
    }

    /// 恢复策略
    pub async fn resume_strategy(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);

        // 1. 检查策略是否处于暂停状态
        let runtime_info = self.get_running_strategy(&strategy_key)
            .ok_or_else(|| StrategyManagerError::StrategyNotRunning {
                strategy_key: strategy_key.clone()
            })?;

        if !matches!(runtime_info.status, StrategyStatus::Paused) {
            return Err(StrategyManagerError::StrategyNotPaused {
                strategy_key: strategy_key.clone()
            }.into());
        }

        let strategy_config_id = runtime_info.strategy_config_id;
        let current_config = runtime_info.get_current_config().await;
        drop(runtime_info);

        // 3. 重新启动策略
        self.start_strategy(strategy_config_id, inst_id.to_string(), period.to_string())
            .await?;

        // 4. 更新状态
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.status = StrategyStatus::Running;
            entry.last_update_time = chrono::Utc::now().timestamp_millis();
        }

        info!("策略已恢复: {}", strategy_key);
        Ok(())
    }

    /// 获取所有运行中的策略
    pub async fn get_running_strategies(&self) -> Vec<SerializableStrategyRuntimeInfo> {
        let mut result = Vec::new();
        for entry in self.running_strategies.iter() {
            result.push(entry.value().to_serializable().await);
        }
        result
    }

    /// 获取指定策略的运行信息
    pub async fn get_strategy_info(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Option<SerializableStrategyRuntimeInfo> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        if let Some(entry) = self.running_strategies.get(&strategy_key) {
            Some(entry.value().to_serializable().await)
        } else {
            None
        }
    }

    /// 批量启动策略
    pub async fn batch_start_strategies(
        &self,
        strategy_configs: Vec<(i64, String, String)>, // (config_id, inst_id, period)
    ) -> Result<BatchOperationResult> {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for (config_id, inst_id, period) in strategy_configs {
            match self
                .start_strategy(config_id, inst_id.clone(), period.clone())
                .await
            {
                Ok(_) => success.push(format!("{}_{}", inst_id, period)),
                Err(e) => failed.push(format!("{}_{}: {}", inst_id, period, e)),
            }
        }

        Ok(BatchOperationResult { success, failed })
    }

    /// 批量停止策略
    pub async fn batch_stop_strategies(
        &self,
        strategies: Vec<(String, String, String)>, // (inst_id, period, strategy_type)
    ) -> Result<BatchOperationResult> {
        let mut success = Vec::new();
        let mut failed = Vec::new();

        for (inst_id, period, strategy_type) in strategies {
            let key = Self::build_strategy_key(&inst_id, &period, &strategy_type);
            match self.stop_strategy(&inst_id, &period, &strategy_type).await {
                Ok(_) => success.push(key),
                Err(e) => failed.push(format!("{}: {}", key, e)),
            }
        }

        Ok(BatchOperationResult { success, failed })
    }

    /// 热更新策略配置（不重启策略）
    pub async fn hot_update_strategy_config(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
        new_config: StrategyConfig,
    ) -> Result<()> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        info!("热更新策略配置: {}", strategy_key);

        // 检查策略是否在运行
        let runtime_info = self
            .running_strategies
            .get(&strategy_key)
            .ok_or_else(|| anyhow!("策略未运行: {}", strategy_key))?;

        // 验证新配置
        if new_config.strategy_config_id <= 0 {
            return Err(anyhow!("策略配置ID必须大于0"));
        }

        // 热更新配置
        runtime_info.hot_update_config(new_config).await?;

        // 更新版本号
        if let Some(mut entry) = self.running_strategies.get_mut(&strategy_key) {
            entry.increment_version();
        }

        // 记录热更新指标
        let metrics = get_strategy_metrics();
        metrics.record_hot_update(&strategy_key).await;

        info!("策略配置热更新成功: {}", strategy_key);
        Ok(())
    }

    /// 获取策略当前配置（用于只读访问）
    pub async fn get_strategy_current_config(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
    ) -> Option<StrategyConfig> {
        let strategy_key = Self::build_strategy_key(inst_id, period, strategy_type);
        if let Some(entry) = self.running_strategies.get(&strategy_key) {
            Some(entry.value().get_current_config().await)
        } else {
            None
        }
    }

    /// 停止所有策略
    pub async fn stop_all_strategies(&self) -> Result<usize> {
        let strategies: Vec<_> = self
            .running_strategies
            .iter()
            .map(|entry| {
                let info = entry.value();
                (
                    info.inst_id.clone(),
                    info.period.clone(),
                    info.strategy_type.clone(),
                )
            })
            .collect();

        let count = strategies.len();
        if count == 0 {
            return Ok(0);
        }

        // 并行停止策略，提升关闭速度；即使个别失败也继续
        let futures = strategies.into_iter().map(|(inst_id, period, strategy_type)| {
            async move {
                if let Err(e) = self.stop_strategy(&inst_id, &period, &strategy_type).await {
                    error!("停止策略失败: {}", e);
                }
            }
        });
        futures::future::join_all(futures).await;

        Ok(count)
    }

    /// 获取系统健康状态
    pub async fn get_system_health(&self) -> crate::trading::services::strategy_metrics::SystemHealth {
        let metrics = get_strategy_metrics();
        metrics.get_system_health(self).await
    }
}

/// 更新策略配置请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStrategyConfigRequest {
    /// 新的策略配置（可选）
    pub strategy_config: Option<String>,
    /// 新的风险配置（可选）
    pub risk_config: Option<String>,
}

/// 批量操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    /// 成功的策略列表
    pub success: Vec<String>,
    /// 失败的策略列表及原因
    pub failed: Vec<String>,
}

// 单例实例
static STRATEGY_MANAGER: once_cell::sync::OnceCell<Arc<StrategyManager>> =
    once_cell::sync::OnceCell::new();

/// 获取策略管理器单例
pub fn get_strategy_manager() -> Arc<StrategyManager> {
    STRATEGY_MANAGER
        .get_or_init(|| Arc::new(StrategyManager::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_strategy_manager_creation() {
        let manager = StrategyManager::new();
        let strategies = manager.get_running_strategies().await;
        assert_eq!(strategies.len(), 0);
    }

    #[tokio::test]
    async fn test_strategy_key_generation() {
        let key = StrategyManager::build_strategy_key("BTC-USDT-SWAP", "1H", "Vegas");
        assert_eq!(key, "Vegas_BTC-USDT-SWAP_1H");
    }
}
