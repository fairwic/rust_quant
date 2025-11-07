use anyhow::Result;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::info;

/// 策略测试进度跟踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyTestProgress {
    pub inst_id: String,
    pub time: String,
    pub config_hash: String,           // 配置的哈希值，用于检测配置是否变化
    pub total_combinations: usize,     // 总参数组合数
    pub completed_combinations: usize, // 已完成的参数组合数
    pub current_index: usize,          // 当前处理的索引
    pub last_update_time: i64,         // 最后更新时间
    pub status: String,                // 状态：running, completed, paused, error
}

/// NWE 随机策略测试配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NweRandomStrategyConfig {
    pub rsi_periods: Vec<usize>,
    pub rsi_over_buy_sell: Vec<(f64, f64)>,
    pub atr_periods: Vec<usize>,
    pub atr_multipliers: Vec<f64>,
    pub volume_bar_nums: Vec<usize>,
    pub volume_ratios: Vec<f64>,
    pub nwe_periods: Vec<usize>,
    pub nwe_multi: Vec<f64>,
    pub batch_size: usize,
    // 风险参数（对齐 Vegas 随机参数生成）
    pub max_loss_percent: Vec<f64>,
    pub take_profit_ratios: Vec<f64>,
    pub is_move_stop_loss: Vec<bool>,
    pub is_used_signal_k_line_stop_loss: Vec<bool>,
}

impl NweRandomStrategyConfig {
    /// 计算配置的哈希值
    pub fn calculate_hash(&self) -> String {
        let config_json = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        config_json.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// 计算总的参数组合数
    pub fn calculate_total_combinations(&self) -> usize {
        self.rsi_periods.len()
            * self.rsi_over_buy_sell.len()
            * self.atr_periods.len()
            * self.atr_multipliers.len()
            * self.volume_bar_nums.len()
            * self.volume_ratios.len()
            * self.nwe_periods.len()
            * self.nwe_multi.len()
            * self.max_loss_percent.len()
            * self.take_profit_ratios.len()
            * self.is_move_stop_loss.len()
            * self.is_used_signal_k_line_stop_loss.len()
    }
}

/// 随机策略测试配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RandomStrategyConfig {
    pub bb_periods: Vec<i32>,
    pub bb_multipliers: Vec<f64>,
    pub shadow_ratios: Vec<f64>,
    pub volume_bar_nums: Vec<usize>,
    pub volume_ratios: Vec<f64>,
    pub breakthrough_thresholds: Vec<f64>,
    pub rsi_periods: Vec<usize>,
    pub rsi_over_buy_sell: Vec<(f64, f64)>,
    pub batch_size: usize,
    //risk
    pub max_loss_percent: Vec<f64>,
    pub take_profit_ratios: Vec<f64>,
    pub is_move_stop_loss: Vec<bool>,
    pub is_used_signal_k_line_stop_loss: Vec<bool>,
}

impl Default for RandomStrategyConfig {
    fn default() -> Self {
        Self {
            bb_periods: vec![10, 11, 12, 13, 14, 15, 16],
            bb_multipliers: vec![2.0, 2.5, 3.0, 3.1, 3.2],
            shadow_ratios: vec![0.65, 0.7, 0.75, 0.8, 0.85, 0.9],
            volume_bar_nums: vec![4, 5, 6, 7],
            volume_ratios: (16..=25).map(|x| x as f64 * 0.1).collect(),
            breakthrough_thresholds: vec![0.003],
            rsi_periods: vec![8, 9, 10, 11, 12, 13, 14, 15, 16],
            rsi_over_buy_sell: vec![(75.0, 25.0), (80.0, 20.0), (85.0, 15.0), (90.0, 10.0)],
            batch_size: 100,
            //risk
            max_loss_percent: vec![0.03, 0.04, 0.05],
            take_profit_ratios: vec![0.0, 1.0, 1.5, 1.8, 2.0, 2.2, 2.4],
            is_move_stop_loss: vec![false, true],
            is_used_signal_k_line_stop_loss: vec![true, false],
        }
    }
}

impl RandomStrategyConfig {
    /// 计算配置的哈希值，用于检测配置是否变化
    pub fn calculate_hash(&self) -> String {
        let config_json = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        config_json.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// 计算总的参数组合数
    pub fn calculate_total_combinations(&self) -> usize {
        self.bb_periods.len()
            * self.bb_multipliers.len()
            * self.shadow_ratios.len()
            * self.volume_bar_nums.len()
            * self.volume_ratios.len()
            * self.breakthrough_thresholds.len()
            * self.rsi_periods.len()
            * self.rsi_over_buy_sell.len()
            * self.max_loss_percent.len()
            * self.take_profit_ratios.len()
            * self.is_move_stop_loss.len()
            * self.is_used_signal_k_line_stop_loss.len()
    }
}

/// 进度管理器
pub struct StrategyProgressManager;

impl StrategyProgressManager {
    /// 获取进度键
    fn get_progress_key(inst_id: &str, time: &str) -> String {
        format!("strategy_progress:{}:{}", inst_id, time)
    }

    /// 保存进度到 Redis
    pub async fn save_progress(progress: &StrategyTestProgress) -> Result<()> {
        let mut redis_conn = rust_quant_core::cache::get_redis_connection().await?;
        let key = Self::get_progress_key(&progress.inst_id, &progress.time);
        let progress_json = serde_json::to_string(progress)?;

        redis_conn.set_ex::<_, _, ()>(&key, progress_json, 86400 * 7).await?; // 保存7天
        info!(
            "进度已保存: {} - {}/{}",
            key, progress.completed_combinations, progress.total_combinations
        );
        Ok(())
    }

    /// 从 Redis 加载进度
    pub async fn load_progress(inst_id: &str, time: &str) -> Result<Option<StrategyTestProgress>> {
        let mut redis_conn = rust_quant_core::cache::get_redis_connection().await?;
        let key = Self::get_progress_key(inst_id, time);

        let progress_json: Option<String> = redis_conn.get(&key).await?;
        if let Some(json) = progress_json {
            let progress: StrategyTestProgress = serde_json::from_str(&json)?;
            info!(
                "进度已加载: {} - {}/{}",
                key, progress.completed_combinations, progress.total_combinations
            );
            Ok(Some(progress))
        } else {
            Ok(None)
        }
    }

    /// 检查配置是否变化
    pub fn is_config_changed(
        current_config: &RandomStrategyConfig,
        saved_progress: &StrategyTestProgress,
    ) -> bool {
        let current_hash = current_config.calculate_hash();
        current_hash != saved_progress.config_hash
    }

    /// 检查 NWE 配置是否变化
    pub fn is_config_changed_nwe(
        current_config: &NweRandomStrategyConfig,
        saved_progress: &StrategyTestProgress,
    ) -> bool {
        let current_hash = current_config.calculate_hash();
        current_hash != saved_progress.config_hash
    }

    /// 创建新的进度记录
    pub fn create_new_progress(
        inst_id: &str,
        time: &str,
        config: &RandomStrategyConfig,
    ) -> StrategyTestProgress {
        StrategyTestProgress {
            inst_id: inst_id.to_string(),
            time: time.to_string(),
            config_hash: config.calculate_hash(),
            total_combinations: config.calculate_total_combinations(),
            completed_combinations: 0,
            current_index: 0,
            last_update_time: chrono::Utc::now().timestamp_millis(),
            status: "running".to_string(),
        }
    }

    /// 创建新的进度记录（NWE）
    pub fn create_new_progress_nwe(
        inst_id: &str,
        time: &str,
        config: &NweRandomStrategyConfig,
    ) -> StrategyTestProgress {
        StrategyTestProgress {
            inst_id: inst_id.to_string(),
            time: time.to_string(),
            config_hash: config.calculate_hash(),
            total_combinations: config.calculate_total_combinations(),
            completed_combinations: 0,
            current_index: 0,
            last_update_time: chrono::Utc::now().timestamp_millis(),
            status: "running".to_string(),
        }
    }

    /// 更新进度
    pub async fn update_progress(
        inst_id: &str,
        time: &str,
        completed_count: usize,
        current_index: usize,
    ) -> Result<()> {
        if let Ok(Some(mut progress)) = Self::load_progress(inst_id, time).await {
            progress.completed_combinations = completed_count;
            progress.current_index = current_index;
            progress.last_update_time = chrono::Utc::now().timestamp_millis();
            Self::save_progress(&progress).await?;
        }
        Ok(())
    }

    /// 标记完成
    pub async fn mark_completed(inst_id: &str, time: &str) -> Result<()> {
        if let Ok(Some(mut progress)) = Self::load_progress(inst_id, time).await {
            progress.status = "completed".to_string();
            progress.completed_combinations = progress.total_combinations;
            progress.current_index = progress.total_combinations;
            progress.last_update_time = chrono::Utc::now().timestamp_millis();
            Self::save_progress(&progress).await?;
            info!("[断点续传] 测试已标记为完成: {}:{}", inst_id, time);
        }
        Ok(())
    }

    /// 清除进度（重新开始）
    pub async fn clear_progress(inst_id: &str, time: &str) -> Result<()> {
        let mut redis_conn = rust_quant_core::cache::get_redis_connection().await?;
        let key = Self::get_progress_key(inst_id, time);
        redis_conn.del::<_, ()>(&key).await?;
        info!("[断点续传] 进度已清除: {}", key);
        Ok(())
    }

    /// 获取进度百分比
    pub fn get_progress_percentage(progress: &StrategyTestProgress) -> f64 {
        if progress.total_combinations == 0 {
            0.0
        } else {
            (progress.completed_combinations as f64 / progress.total_combinations as f64) * 100.0
        }
    }

    /// 估算剩余时间（基于已用时间和完成进度）
    pub fn estimate_remaining_time(
        progress: &StrategyTestProgress,
        start_time: i64,
    ) -> Option<i64> {
        if progress.completed_combinations == 0 {
            return None;
        }

        let elapsed_time = chrono::Utc::now().timestamp_millis() - start_time;
        let progress_ratio =
            progress.completed_combinations as f64 / progress.total_combinations as f64;

        if progress_ratio > 0.0 {
            let estimated_total_time = elapsed_time as f64 / progress_ratio;
            let remaining_time = estimated_total_time - elapsed_time as f64;
            Some(remaining_time.max(0.0) as i64)
        } else {
            None
        }
    }
}
