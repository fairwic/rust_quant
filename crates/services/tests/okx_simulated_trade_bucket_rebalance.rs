use anyhow::Result;
use okx::enums::account_enums::AccountType;
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_services::exchange::OkxOrderService;

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("missing env var: {}", key))
}

fn build_simulated_api_config() -> anyhow::Result<ExchangeApiConfig> {
    Ok(ExchangeApiConfig::new(
        0,
        "okx".to_string(),
        env_required("OKX_SIMULATED_API_KEY")?,
        env_required("OKX_SIMULATED_API_SECRET")?,
        Some(env_required("OKX_SIMULATED_PASSPHRASE")?),
        true,
        true,
        Some("integration-test".to_string()),
    ))
}

fn compute_transfer(
    trade_balance: f64,
    funding_balance: f64,
    target_trade_ratio: f64,
    min_transfer: f64,
    epsilon: f64,
) -> Option<(f64, AccountType, AccountType, f64)> {
    let total_balance = trade_balance + funding_balance;
    if total_balance <= 0.0 {
        return None;
    }

    let target_trade_balance = total_balance * target_trade_ratio;
    let diff = target_trade_balance - trade_balance;
    if diff.abs() < epsilon {
        return None;
    }

    if diff > 0.0 && diff >= min_transfer {
        return Some((
            diff,
            AccountType::FOUND,
            AccountType::TRADE,
            target_trade_balance,
        ));
    }

    if diff < 0.0 && (-diff) >= min_transfer {
        return Some((
            -diff,
            AccountType::TRADE,
            AccountType::FOUND,
            target_trade_balance,
        ));
    }

    None
}

#[tokio::test]
#[ignore]
async fn okx_simulated_trade_bucket_rebalance_flow() -> Result<()> {
    dotenv::dotenv().ok();
    std::env::set_var("OKX_REQUEST_EXPIRATION_MS", "300000");

    if env_or_default("RUN_OKX_SIMULATED_REBALANCE_E2E", "0") != "1" {
        return Ok(());
    }

    if env_or_default("APP_ENV", "local").eq_ignore_ascii_case("prod") {
        anyhow::bail!("refuse to run in APP_ENV=prod");
    }

    let currency = env_or_default("LIVE_TRADE_BUCKET_CURRENCY", "USDT");
    let target_trade_ratio: f64 = env_or_default("LIVE_TARGET_TRADE_RATIO", "0.30")
        .parse()
        .unwrap_or(0.30);
    let min_transfer: f64 = env_or_default("LIVE_TRADE_BUCKET_MIN_TRANSFER_USDT", "1")
        .parse()
        .unwrap_or(1.0);
    let epsilon: f64 = env_or_default("LIVE_TRADE_BUCKET_EPSILON_USDT", "0.5")
        .parse()
        .unwrap_or(0.5);

    let api = build_simulated_api_config()?;
    api.validate().map_err(|e| anyhow::anyhow!(e))?;
    let okx = OkxOrderService;

    let positions = okx.get_positions(&api, Some("SWAP"), None).await?;
    let has_open_swap_positions = positions
        .iter()
        .any(|p| p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12);
    if has_open_swap_positions {
        anyhow::bail!("refuse to rebalance while swap positions are still open");
    }

    let force_drift: f64 = env_or_default("OKX_REBALANCE_TEST_FORCE_DRIFT_USDT", "0")
        .parse()
        .unwrap_or(0.0);

    let mut funding_before = okx.get_funding_available_balance(&api, &currency).await?;
    let mut trade_before = okx.get_trade_available_equity(&api, &currency).await?;
    let total_before = funding_before + trade_before;
    let target_trade_before = total_before * target_trade_ratio;

    println!(
        "before: funding={:.8}, trade={:.8}, total={:.8}, target_trade={:.8}",
        funding_before, trade_before, total_before, target_trade_before
    );

    if force_drift > 0.0 {
        println!(
            "inject drift: amount={:.8}, from={:?}, to={:?}",
            force_drift,
            AccountType::TRADE,
            AccountType::FOUND
        );
        let drift_res = okx
            .transfer_between_accounts(
                &api,
                &currency,
                force_drift,
                AccountType::TRADE,
                AccountType::FOUND,
            )
            .await?;
        println!("drift_res={}", serde_json::to_string_pretty(&drift_res)?);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        funding_before = okx.get_funding_available_balance(&api, &currency).await?;
        trade_before = okx.get_trade_available_equity(&api, &currency).await?;
        let total_after_drift = funding_before + trade_before;
        let target_after_drift = total_after_drift * target_trade_ratio;
        println!(
            "after drift: funding={:.8}, trade={:.8}, total={:.8}, target_trade={:.8}",
            funding_before, trade_before, total_after_drift, target_after_drift
        );
    }

    let Some((amount, from, to, target_trade_balance)) = compute_transfer(
        trade_before,
        funding_before,
        target_trade_ratio,
        min_transfer,
        epsilon,
    ) else {
        println!(
            "skip: no transfer needed (epsilon/min_transfer), ratio={}, epsilon={}, min_transfer={}",
            target_trade_ratio, epsilon, min_transfer
        );
        return Ok(());
    };

    println!(
        "transfer plan: amount={:.8}, from={:?}, to={:?}, target_trade={:.8}",
        amount, from, to, target_trade_balance
    );

    let transfer_res = okx
        .transfer_between_accounts(&api, &currency, amount, from, to)
        .await?;
    println!("transfer_res={}", serde_json::to_string_pretty(&transfer_res)?);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let funding_after = okx.get_funding_available_balance(&api, &currency).await?;
    let trade_after = okx.get_trade_available_equity(&api, &currency).await?;
    let total_after = funding_after + trade_after;
    let target_trade_after = total_after * target_trade_ratio;

    println!(
        "after: funding={:.8}, trade={:.8}, total={:.8}, target_trade={:.8}",
        funding_after, trade_after, total_after, target_trade_after
    );

    let before_gap = (trade_before - target_trade_before).abs();
    let after_gap = (trade_after - target_trade_after).abs();
    println!(
        "gap: before={:.8}, after={:.8}, delta={:.8}",
        before_gap,
        after_gap,
        before_gap - after_gap
    );

    anyhow::ensure!(
        after_gap <= before_gap + 1e-6,
        "rebalance did not improve trade balance gap: before={:.8}, after={:.8}",
        before_gap,
        after_gap
    );

    Ok(())
}
