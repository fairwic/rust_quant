use rust_quant::app_init;
use rust_quant::trading::strategy::strategy_manager::{
    get_strategy_manager, UpdateStrategyConfigRequest,
};

/// 测试策略管理器的基本功能
/// 这个测试演示了如何像 API 请求一样调用策略管理功能
#[tokio::test]
async fn test_strategy_manager_basic_operations() {
    // 初始化应用环境（数据库连接等）
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();

    // 测试参数 - 请根据实际数据库中的配置修改
    let config_id = 1_i64;
    let inst_id = "BTC-USDT-SWAP".to_string();
    let period = "1H".to_string();
    let strategy_type = "Vegas".to_string();

    println!("🚀 开始测试策略管理器功能");

    // 1. 启动策略
    println!(
        "📈 启动策略: config_id={}, inst_id={}, period={}",
        config_id, inst_id, period
    );
    match manager
        .start_strategy(config_id, inst_id.clone(), period.clone())
        .await
    {
        Ok(_) => println!("✅ 策略启动成功"),
        Err(e) => {
            println!("❌ 策略启动失败: {}", e);
            // 如果启动失败，可能是因为：
            // 1. 数据库中不存在对应的配置记录
            // 2. 调度器未初始化
            // 3. 策略已在运行
            return;
        }
    }

    // 2. 查询策略状态
    println!("📊 查询策略运行状态");
    match manager
        .get_strategy_info(&inst_id, &period, &strategy_type)
        .await
    {
        Some(info) => {
            println!("✅ 策略信息: {:?}", info);
        }
        None => {
            println!("❌ 策略未运行或不存在");
        }
    }

    // 3. 获取所有运行中的策略
    println!("📋 获取所有运行中的策略");
    let running_strategies = manager.get_running_strategies().await;
    println!("✅ 运行中的策略数量: {}", running_strategies.len());
    for strategy in &running_strategies {
        println!(
            "  - {}_{}_{}，状态: {:?}",
            strategy.strategy_type, strategy.inst_id, strategy.period, strategy.status
        );
    }

    // 4. 暂停策略
    println!("⏸️  暂停策略");
    match manager
        .pause_strategy(&inst_id, &period, &strategy_type)
        .await
    {
        Ok(_) => println!("✅ 策略暂停成功"),
        Err(e) => println!("❌ 策略暂停失败: {}", e),
    }

    // 5. 恢复策略
    println!("▶️  恢复策略");
    match manager
        .resume_strategy(&inst_id, &period, &strategy_type)
        .await
    {
        Ok(_) => println!("✅ 策略恢复成功"),
        Err(e) => println!("❌ 策略恢复失败: {}", e),
    }

    // 6. 更新策略配置
    println!("🔧 更新策略配置");
    let update_req = UpdateStrategyConfigRequest {
        strategy_config: Some(r#"{"period":"1H","min_k_line_num":7000}"#.to_string()),
        risk_config: Some(r#"{"max_position_ratio":0.5,"stop_loss_ratio":0.02}"#.to_string()),
    };
    match manager
        .update_strategy_config(&inst_id, &period, &strategy_type, update_req)
        .await
    {
        Ok(_) => println!("✅ 策略配置更新成功"),
        Err(e) => println!("❌ 策略配置更新失败: {}", e),
    }

    // 7. 停止策略
    println!("🛑 停止策略");
    match manager
        .stop_strategy(&inst_id, &period, &strategy_type)
        .await
    {
        Ok(_) => println!("✅ 策略停止成功"),
        Err(e) => println!("❌ 策略停止失败: {}", e),
    }

    println!("🎉 策略管理器功能测试完成");
}

/// 测试批量操作
#[tokio::test]
async fn test_batch_operations() {
    // 初始化应用环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();

    println!("🔄 测试批量操作");

    // 批量启动策略
    let strategies_to_start = vec![
        (1_i64, "BTC-USDT-SWAP".to_string(), "1H".to_string()),
        (2_i64, "ETH-USDT-SWAP".to_string(), "4H".to_string()),
    ];

    match manager.batch_start_strategies(strategies_to_start).await {
        Ok(result) => {
            println!("✅ 批量启动完成");
            println!("  成功: {:?}", result.success);
            println!("  失败: {:?}", result.failed);
        }
        Err(e) => println!("❌ 批量启动失败: {}", e),
    }

    // 等待一段时间让策略运行
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 停止所有策略
    match manager.stop_all_strategies().await {
        Ok(count) => println!("✅ 停止了 {} 个策略", count),
        Err(e) => println!("❌ 停止所有策略失败: {}", e),
    }
}

/// 测试错误场景
#[tokio::test]
async fn test_error_scenarios() {
    // 初始化应用环境
    if let Err(e) = app_init().await {
        eprintln!("应用初始化失败: {}", e);
        return;
    }

    let manager = get_strategy_manager();

    println!("🧪 测试错误场景");

    // 1. 启动不存在的策略配置
    match manager
        .start_strategy(99999_i64, "INVALID-SWAP".to_string(), "1H".to_string())
        .await
    {
        Ok(_) => println!("❌ 预期失败但成功了"),
        Err(e) => println!("✅ 预期的错误: {}", e),
    }

    // 2. 停止不存在的策略
    match manager.stop_strategy("INVALID-SWAP", "1H", "Vegas").await {
        Ok(_) => println!("❌ 预期失败但成功了"),
        Err(e) => println!("✅ 预期的错误: {}", e),
    }

    // 3. 查询不存在的策略
    match manager
        .get_strategy_info("INVALID-SWAP", "1H", "Vegas")
        .await
    {
        Some(_) => println!("❌ 预期返回 None 但返回了数据"),
        None => println!("✅ 正确返回 None"),
    }

    println!("🎯 错误场景测试完成");
}
