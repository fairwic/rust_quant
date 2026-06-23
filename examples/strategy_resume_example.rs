use rust_quant::app_config::{log, redis_config};
use rust_quant::trading::task::basic::{
    RandomStrategyConfig, StrategyProgressManager,
    test_random_strategy_with_config, back_test_with_config, BackTestConfig
};
use tokio::sync::Semaphore;
use std::sync::Arc;
use tracing::{info, warn};
/// 策略断点续传使用示例
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置环境变量
    std::env::set_var("APP_ENV", "local");
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:6379");
    // 初始化 Redis 连接池
    redis_config::init_redis_pool().await?;
    // 初始化日志
    log::setup_logging().await?;
    info!("🚀 策略断点续传示例开始");
    // 示例1: 查看现有进度
    let inst_id = "BTC-USDT";
    let time = "1H";
    match StrategyProgressManager::load_progress(inst_id, time).await {
        Ok(Some(progress)) => {
            let percentage = StrategyProgressManager::get_progress_percentage(&progress);
            info!(
                "📊 发现现有进度: {}/{} ({:.2}%), 状态: {}",
                progress.completed_combinations,
                progress.total_combinations,
                percentage,
                progress.status
            );
            if progress.status == "completed" {
                info!("✅ 测试已完成，如需重新测试请先清除进度");
                return Ok(());
            }
        }
        Ok(None) => {
            info!("📝 未发现现有进度，将开始新的测试");
        }
        Err(e) => {
            warn!("⚠️ 加载进度失败: {}", e);
        }
    }
    // 示例2: 配置策略测试参数
    let config = RandomStrategyConfig {
        bb_periods: vec![10, 11, 12],
        bb_multipliers: vec![2.0, 2.5, 3.0],
        shadow_ratios: vec![0.7, 0.8, 0.9],
        volume_bar_nums: vec![4, 5, 6],
        volume_ratios: vec![1.6, 1.8, 2.0],
        breakthrough_thresholds: vec![0.003],
        rsi_periods: vec![8, 10, 12],
        rsi_over_buy_sell: vec![(85.0, 15.0), (90.0, 10.0)],
        batch_size: 50, // 批量大小，可根据系统性能调整
        max_loss_percent: vec![0.03, 0.05, 0.08],
        take_profit_ratios: vec![0.0],
        is_used_signal_k_line_stop_loss: vec![true, false],
        k_line_hammer_shadow_ratios: vec![0.65, 0.75],
        fix_signal_kline_take_profit_ratios: vec![0.0],
    };
    let total_combinations = config.calculate_total_combinations();
    info!("📊 策略配置总组合数: {}", total_combinations);
    info!("🔧 配置哈希: {}", config.calculate_hash());
    // 示例3: 创建信号量控制并发
    let max_concurrent = 10; // 根据系统性能调整
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    // 示例4: 执行策略测试（支持断点续传）
    info!("🔄 开始执行策略测试（支持断点续传）");
    match test_random_strategy_with_config(inst_id, time, semaphore, config).await {
        Ok(()) => {
            info!("🎉 策略测试完成！");
            // 查看最终进度
            if let Ok(Some(final_progress)) = StrategyProgressManager::load_progress(inst_id, time).await {
                let percentage = StrategyProgressManager::get_progress_percentage(&final_progress);
                info!(
                    "📈 最终进度: {}/{} ({:.2}%), 状态: {}",
                    final_progress.completed_combinations,
                    final_progress.total_combinations,
                    percentage,
                    final_progress.status
                );
            }
        }
        Err(e) => {
            warn!("❌ 策略测试失败: {}", e);
            // 查看当前进度
            if let Ok(Some(current_progress)) = StrategyProgressManager::load_progress(inst_id, time).await {
                let percentage = StrategyProgressManager::get_progress_percentage(&current_progress);
                info!(
                    "📊 当前进度: {}/{} ({:.2}%), 可稍后继续",
                    current_progress.completed_combinations,
                    current_progress.total_combinations,
                    percentage
                );
            }
        }
    }
    Ok(())
}
/// 进度管理工具函数示例
async fn progress_management_examples() -> Result<(), Box<dyn std::error::Error>> {
    let inst_id = "ETH-USDT";
    let time = "4H";
    // 1. 清除进度（重新开始）
    info!("🧹 清除进度示例");
    StrategyProgressManager::clear_progress(inst_id, time).await?;
    // 2. 创建新进度
    info!("📝 创建新进度示例");
    let config = RandomStrategyConfig::default();
    let new_progress = StrategyProgressManager::create_new_progress(inst_id, time, &config);
    StrategyProgressManager::save_progress(&new_progress).await?;
    // 3. 更新进度
    info!("📈 更新进度示例");
    StrategyProgressManager::update_progress(inst_id, time, 100, 100).await?;
    // 4. 查看进度
    info!("👀 查看进度示例");
    if let Ok(Some(progress)) = StrategyProgressManager::load_progress(inst_id, time).await {
        let percentage = StrategyProgressManager::get_progress_percentage(&progress);
        info!("当前进度: {:.2}%", percentage);
        // 估算剩余时间
        let start_time = chrono::Utc::now().timestamp_millis() - 60000; // 假设1分钟前开始
        if let Some(remaining_ms) = StrategyProgressManager::estimate_remaining_time(&progress, start_time) {
            let remaining_minutes = remaining_ms / 1000 / 60;
            info!("预计剩余时间: {} 分钟", remaining_minutes);
        }
    }
    // 5. 标记完成
    info!("✅ 标记完成示例");
    StrategyProgressManager::mark_completed(inst_id, time).await?;
    Ok(())
}
/// 配置变化检测示例
async fn config_change_detection_example() -> Result<(), Box<dyn std::error::Error>> {
    let inst_id = "BTC-USDT";
    let time = "1H";
    // 原始配置
    let original_config = RandomStrategyConfig::default();
    let progress = StrategyProgressManager::create_new_progress(inst_id, time, &original_config);
    StrategyProgressManager::save_progress(&progress).await?;
    // 修改配置
    let mut modified_config = original_config.clone();
    modified_config.bb_periods = vec![20, 21, 22]; // 修改参数
    // 检测变化
    if StrategyProgressManager::is_config_changed(&modified_config, &progress) {
        info!("🔄 检测到配置变化，将重新开始测试");
        info!("原始哈希: {}", progress.config_hash);
        info!("新配置哈希: {}", modified_config.calculate_hash());
    } else {
        info!("✅ 配置未变化，可以继续之前的测试");
    }
    Ok(())
}
/// 批量测试示例
async fn batch_testing_example() -> Result<(), Box<dyn std::error::Error>> {
    let instruments = vec!["BTC-USDT", "ETH-USDT", "SOL-USDT"];
    let timeframes = vec!["1H", "4H", "1D"];
    for inst_id in &instruments {
        for time in &timeframes {
            info!("🔄 开始测试 {} - {}", inst_id, time);
            // 检查是否已完成
            if let Ok(Some(progress)) = StrategyProgressManager::load_progress(inst_id, time).await {
                if progress.status == "completed" {
                    info!("✅ {} - {} 已完成，跳过", inst_id, time);
                    continue;
                }
            }
            // 执行测试
            let config = RandomStrategyConfig::default();
            let semaphore = Arc::new(Semaphore::new(5));
            match test_random_strategy_with_config(inst_id, time, semaphore, config).await {
                Ok(()) => info!("✅ {} - {} 测试完成", inst_id, time),
                Err(e) => warn!("❌ {} - {} 测试失败: {}", inst_id, time, e),
            }
        }
    }
    Ok(())
}
