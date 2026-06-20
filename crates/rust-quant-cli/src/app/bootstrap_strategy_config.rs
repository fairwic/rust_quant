/// 创建策略配置服务实例（依赖注入）
fn create_strategy_config_service() -> Result<StrategyConfigService> {
    validate_strategy_config_source()?;
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL").context(
        "策略配置固定使用 quant_core.strategy_configs，必须设置 QUANT_CORE_DATABASE_URL",
    )?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)
        .context("创建 quant_core Postgres strategy_configs 连接池失败")?;
    info!("📚 策略配置来源: quant_core.strategy_configs");
    Ok(StrategyConfigService::new(Box::new(
        PostgresStrategyConfigRepository::new(pool),
    )))
}

fn validate_strategy_config_source() -> Result<()> {
    let source = std::env::var("STRATEGY_CONFIG_SOURCE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if source.is_empty() || source == "quant_core" || source == "postgres" {
        return Ok(());
    }
    if source == "strategy_config" || source == "legacy_pg" {
        return Err(anyhow!(
            "STRATEGY_CONFIG_SOURCE={} 已废弃；策略配置只保留 quant_core.strategy_configs",
            source
        ));
    }
    Err(anyhow!("不支持的 STRATEGY_CONFIG_SOURCE: {}", source))
}

async fn load_backtest_targets_from_db() -> Result<Vec<(String, String)>> {
    let service = create_strategy_config_service()?;
    let configs = service.load_all_enabled_configs().await?;

    let targets: Vec<(String, String)> = configs
        .into_iter()
        .filter(|cfg| cfg.strategy_type == StrategyType::Nwe)
        .map(|cfg| (cfg.symbol.clone(), cfg.timeframe.as_str().to_string()))
        .collect();

    if targets.is_empty() {
        return Err(anyhow!("未找到启用的 NWE 策略配置"));
    }

    Ok(targets)
}
