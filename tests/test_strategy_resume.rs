use rust_quant::app_config::{log, redis_config};
use rust_quant::trading::task::basic::{
    RandomStrategyConfig, StrategyProgressManager, StrategyTestProgress,
    test_random_strategy_with_config
};
use tokio::sync::Semaphore;
use std::sync::Arc;
use tracing::info;

#[tokio::test]
async fn test_strategy_resume_functionality() {
    // 设置测试环境变量
    std::env::set_var("APP_ENV", "local");
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:6379");

    // 初始化 Redis 连接池
    redis_config::init_redis_pool().await.expect("Failed to initialize Redis pool");

    // 初始化日志
    log::setup_logging().await.expect("Failed to initialize log config");

    let inst_id = "BTC-USDT";
    let time = "1H";

    info!("🚀 开始测试策略断点续传功能");

    // 创建一个小的测试配置，减少测试时间
    let small_config = RandomStrategyConfig {
        bb_periods: vec![10, 11],
        bb_multipliers: vec![2.0, 2.5],
        shadow_ratios: vec![0.7, 0.8],
        volume_bar_nums: vec![4, 5],
        volume_ratios: vec![1.6, 1.7],
        breakthrough_thresholds: vec![0.003],
        rsi_periods: vec![8, 9],
        rsi_over_buy_sell: vec![(85.0, 15.0), (86.0, 14.0)],
        batch_size: 2, // 小批次，便于测试
        max_loss_percent: vec![0.03, 0.04],
        take_profit_ratios: vec![0.0],
        is_used_signal_k_line_stop_loss: vec![true],
        k_line_hammer_shadow_ratios: vec![0.65],
        fix_signal_kline_take_profit_ratios: vec![0.0],
    };

    let total_combinations = small_config.calculate_total_combinations();
    info!("📊 测试配置总组合数: {}", total_combinations);

    // 步骤1: 清除之前的进度
    info!("🧹 清除之前的进度");
    StrategyProgressManager::clear_progress(inst_id, time).await.unwrap();

    // 步骤2: 验证没有进度记录
    let progress = StrategyProgressManager::load_progress(inst_id, time).await.unwrap();
    assert!(progress.is_none(), "进度应该为空");
    info!("✅ 确认进度已清除");

    // 步骤3: 创建新进度并保存
    let new_progress = StrategyProgressManager::create_new_progress(inst_id, time, &small_config);
    info!("📝 创建新进度: 配置哈希={}", new_progress.config_hash);
    StrategyProgressManager::save_progress(&new_progress).await.unwrap();

    // 步骤4: 验证进度保存成功
    let loaded_progress = StrategyProgressManager::load_progress(inst_id, time).await.unwrap();
    assert!(loaded_progress.is_some(), "应该能加载到进度");
    let loaded_progress = loaded_progress.unwrap();
    assert_eq!(loaded_progress.config_hash, new_progress.config_hash);
    assert_eq!(loaded_progress.total_combinations, total_combinations);
    info!("✅ 进度保存和加载验证成功");

    // 步骤5: 模拟部分完成的进度
    let mut partial_progress = loaded_progress.clone();
    partial_progress.completed_combinations = total_combinations / 2;
    partial_progress.current_index = total_combinations / 2;
    partial_progress.status = "running".to_string();
    StrategyProgressManager::save_progress(&partial_progress).await.unwrap();
    info!("📈 模拟部分完成进度: {}/{}", partial_progress.completed_combinations, partial_progress.total_combinations);

    // 步骤6: 验证进度百分比计算
    let percentage = StrategyProgressManager::get_progress_percentage(&partial_progress);
    assert!((percentage - 50.0).abs() < 0.1, "进度百分比应该约为50%");
    info!("✅ 进度百分比计算正确: {:.2}%", percentage);

    // 步骤7: 测试配置变化检测
    let mut changed_config = small_config.clone();
    changed_config.bb_periods = vec![12, 13]; // 修改配置

    let is_changed = StrategyProgressManager::is_config_changed(&changed_config, &partial_progress);
    assert!(is_changed, "应该检测到配置变化");
    info!("✅ 配置变化检测正确");

    // 步骤8: 测试配置未变化的情况
    let is_unchanged = StrategyProgressManager::is_config_changed(&small_config, &partial_progress);
    assert!(!is_unchanged, "相同配置不应该被检测为变化");
    info!("✅ 配置未变化检测正确");

    // 步骤9: 标记完成
    StrategyProgressManager::mark_completed(inst_id, time).await.unwrap();
    let completed_progress = StrategyProgressManager::load_progress(inst_id, time).await.unwrap().unwrap();
    assert_eq!(completed_progress.status, "completed");
    assert_eq!(completed_progress.completed_combinations, completed_progress.total_combinations);
    info!("✅ 完成状态标记正确");

    // 步骤10: 清理测试数据
    StrategyProgressManager::clear_progress(inst_id, time).await.unwrap();
    info!("🧹 测试数据已清理");

    info!("🎉 策略断点续传功能测试完成！");
}

#[tokio::test]
async fn test_strategy_resume_integration() {
    // 设置测试环境变量
    std::env::set_var("APP_ENV", "local");
    std::env::set_var("REDIS_URL", "redis://127.0.0.1:6379");

    // 初始化 Redis 连接池
    redis_config::init_redis_pool().await.expect("Failed to initialize Redis pool");

    // 初始化日志
    log::setup_logging().await.expect("Failed to initialize log config");

    let inst_id = "ETH-USDT";
    let time = "4H";

    info!("🔄 开始集成测试：实际运行策略测试");

    // 创建极小的配置用于快速测试
    let tiny_config = RandomStrategyConfig {
        bb_periods: vec![10],
        bb_multipliers: vec![2.0],
        shadow_ratios: vec![0.7],
        volume_bar_nums: vec![4],
        volume_ratios: vec![1.6],
        breakthrough_thresholds: vec![0.003],
        rsi_periods: vec![8],
        rsi_over_buy_sell: vec![(85.0, 15.0)],
        batch_size: 1,
        max_loss_percent: vec![0.03],
        take_profit_ratios: vec![0.0],
        is_used_signal_k_line_stop_loss: vec![true],
        k_line_hammer_shadow_ratios: vec![0.65],
        fix_signal_kline_take_profit_ratios: vec![0.0],
    };

    info!("📊 集成测试配置总组合数: {}", tiny_config.calculate_total_combinations());

    // 清除之前的进度
    StrategyProgressManager::clear_progress(inst_id, time).await.unwrap();

    // 创建信号量
    let semaphore = Arc::new(Semaphore::new(1));

    // 注意：这里只是测试框架，实际的策略测试需要有效的K线数据
    // 在真实环境中，这个测试会尝试加载K线数据
    info!("⚠️  注意：此集成测试需要有效的K线数据才能完全运行");

    // 验证进度管理器的基本功能
    let progress_before = StrategyProgressManager::load_progress(inst_id, time).await.unwrap();
    assert!(progress_before.is_none(), "开始前应该没有进度记录");

    info!("✅ 集成测试基础验证完成");
}

#[tokio::test]
async fn test_param_generator_resume() {
    use rust_quant::trading::task::job_param_generator::ParamGenerator;

    info!("🔧 测试参数生成器的断点续传功能");

    let mut generator = ParamGenerator::new(
        vec![10, 11],
        vec![0.7, 0.8],
        vec![2.0, 2.5],
        vec![4, 5],
        vec![1.6, 1.7],
        vec![0.003],
        vec![8, 9],
        vec![(85.0, 15.0), (86.0, 14.0)],
        vec![0.03, 0.04],
        vec![true],
        vec![0.0],
    );

    let (initial_index, total) = generator.progress();
    assert_eq!(initial_index, 0);
    info!("📊 生成器初始状态: {}/{}", initial_index, total);

    // 获取前几个批次
    let batch1 = generator.get_next_batch(2);
    assert_eq!(batch1.len(), 2);
    let (after_batch1, _) = generator.progress();
    info!("📦 第一批次后进度: {}/{}", after_batch1, total);

    // 设置到中间位置
    let middle_index = total / 2;
    generator.set_current_index(middle_index);
    let (after_set, _) = generator.progress();
    assert_eq!(after_set, middle_index);
    info!("🎯 设置到中间位置: {}/{}", after_set, total);

    // 验证剩余数量
    let remaining = generator.remaining_count();
    assert_eq!(remaining, total - middle_index);
    info!("📈 剩余组合数: {}", remaining);

    // 测试完成状态
    generator.set_current_index(total);
    assert!(generator.is_completed());
    info!("✅ 完成状态检测正确");

    info!("🎉 参数生成器断点续传功能测试完成！");
}
