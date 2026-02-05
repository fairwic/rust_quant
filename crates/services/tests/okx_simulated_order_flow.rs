use std::time::Duration;

use okx::dto::trade::trade_dto::OrderResDto;
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_risk::realtime::{OkxStopLossAmender, StopLossAmender};
use rust_quant_services::exchange::OkxOrderService;
use rust_quant_strategies::strategy_common::SignalResult;

#[derive(serde::Deserialize)]
struct OkxTickerResponse {
    data: Vec<OkxTickerData>,
}

#[derive(serde::Deserialize)]
struct OkxTickerData {
    last: String,
}

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("missing env var: {}", key))
}

// 获取最新价格
async fn fetch_last_price(inst_id: &str) -> anyhow::Result<f64> {
    let url = format!(
        "https://www.okx.com/api/v5/market/ticker?instId={}",
        inst_id
    );
    let resp = reqwest::get(url).await?.error_for_status()?;
    let data: OkxTickerResponse = resp.json().await?;
    let last_str = data
        .data
        .get(0)
        .ok_or_else(|| anyhow::anyhow!("empty ticker response"))?
        .last
        .trim()
        .to_string();
    let last = last_str
        .parse::<f64>()
        .map_err(|e| anyhow::anyhow!("invalid last price '{}': {}", last_str, e))?;
    Ok(last)
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

fn build_signal(side: &str, open_price: f64) -> SignalResult {
    let ts = chrono::Utc::now().timestamp_millis();
    let (should_buy, should_sell, direction) = match side {
        "buy" => (true, false, rust_quant_domain::SignalDirection::Long),
        "sell" => (false, true, rust_quant_domain::SignalDirection::Short),
        _ => (false, false, rust_quant_domain::SignalDirection::None),
    };

    SignalResult {
        should_buy,
        should_sell,
        open_price,
        signal_kline_stop_loss_price: None,
        best_open_price: None,
        atr_take_profit_ratio_price: None,
        atr_stop_loss_price: None,
        long_signal_take_profit_price: None,
        short_signal_take_profit_price: None,
        move_stop_open_price_when_touch_price: None,
        ts,
        single_value: None,
        single_result: None,
        is_ema_short_trend: None,
        is_ema_long_trend: None,
        atr_take_profit_level_1: None,
        atr_take_profit_level_2: None,
        atr_take_profit_level_3: None,
        filter_reasons: vec![],
        dynamic_adjustments: vec![],
        dynamic_config_snapshot: None,
        stop_loss_source: None,
        direction,
    }
}

fn compute_tp_sl(last: f64, side: &str) -> (Option<f64>, Option<f64>) {
    // Keep triggers far away to avoid immediate fills during the test.
    // Override via env if you want tighter behavior.
    let tp_pct: f64 = env_or_default("OKX_TEST_TP_PCT", "0.10")
        .parse()
        .unwrap_or(0.10);
    let sl_pct: f64 = env_or_default("OKX_TEST_SL_PCT", "0.10")
        .parse()
        .unwrap_or(0.10);

    match side {
        "buy" => (Some(last * (1.0 + tp_pct)), Some(last * (1.0 - sl_pct))),
        "sell" => (Some(last * (1.0 - tp_pct)), Some(last * (1.0 + sl_pct))),
        _ => (None, None),
    }
}

async fn get_position_mgn_mode(
    okx: &OkxOrderService,
    api: &ExchangeApiConfig,
    inst_id: &str,
    pos_side: &str,
) -> anyhow::Result<Option<String>> {
    let positions = okx.get_positions(api, Some("SWAP"), Some(inst_id)).await?;
    for p in positions {
        if p.inst_id != inst_id {
            continue;
        }
        if p.pos_side != pos_side {
            continue;
        }
        let qty = p.pos.parse::<f64>().unwrap_or(0.0);
        if qty.abs() < 1e-12 {
            continue;
        }
        return Ok(Some(p.mgn_mode));
    }
    Ok(None)
}

async fn wait_for_position(
    okx: &OkxOrderService,
    api: &ExchangeApiConfig,
    inst_id: &str,
    pos_side: &str,
    should_exist: bool,
) -> anyhow::Result<()> {
    let max_tries: usize = env_or_default("OKX_TEST_RETRY", "20").parse().unwrap_or(20);
    let sleep_ms: u64 = env_or_default("OKX_TEST_RETRY_SLEEP_MS", "500")
        .parse()
        .unwrap_or(500);

    for _ in 0..max_tries {
        let exists = get_position_mgn_mode(okx, api, inst_id, pos_side)
            .await?
            .is_some();
        println!("exists: {:?}", exists);
        if exists == should_exist {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
    }

    anyhow::bail!(
        "position state did not converge: inst_id={}, pos_side={}, expected_exist={}",
        inst_id,
        pos_side,
        should_exist
    );
}

async fn place_order(
    okx: &OkxOrderService,
    api: &ExchangeApiConfig,
    inst_id: &str,
    side: &str,
    size: String,
    tp: Option<f64>,
    sl: Option<f64>,
) -> anyhow::Result<OrderResDto> {
    let signal = build_signal(side, fetch_last_price(inst_id).await?);
    let cl_ord_id = format!("t{}", chrono::Utc::now().timestamp_millis());

    let res = okx
        .execute_order_from_signal(api, inst_id, &signal, size, sl, tp, Some(cl_ord_id))
        .await?;
    println!("res: {:?}", res);

    let first = res
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty order response"))?;

    if first.s_code != "0" {
        anyhow::bail!(
            "place order failed: s_code={}, s_msg={:?}",
            first.s_code,
            first.s_msg
        );
    }
    if first.ord_id.trim().is_empty() {
        anyhow::bail!("place order returned empty ord_id");
    }

    Ok(first)
}

/// OKX simulated trading end-to-end integration test.
///
/// Run manually:
/// - `RUN_OKX_SIMULATED_E2E=1 cargo test -p rust-quant-services --test okx_simulated_order_flow -- --ignored --nocapture`
///
/// Env knobs:
/// - `OKX_TEST_INST_ID` (default: `ETH-USDT-SWAP`)
/// - `OKX_TEST_SIDE` (default: `buy`, values: `buy`/`sell`)
/// - `OKX_TEST_ORDER_SIZE` (default: `1`)
/// - `OKX_TEST_TP_PCT` / `OKX_TEST_SL_PCT` (default: `0.10`)
/// - `OKX_TEST_RETRY` / `OKX_TEST_RETRY_SLEEP_MS` (polling)
#[tokio::test]
#[ignore]
async fn okx_simulated_order_flow_place_amend_close() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    // 给 OKX 请求稍微多一点时间窗口，避免 local time 比服务器慢导致 expTime 过期。
    std::env::set_var("OKX_REQUEST_EXPIRATION_MS", "300000");

    if env_or_default("RUN_OKX_SIMULATED_E2E", "0") != "1" {
        return Ok(());
    }

    // Safety: do not run under prod
    if env_or_default("APP_ENV", "local").eq_ignore_ascii_case("prod") {
        anyhow::bail!("refuse to run in APP_ENV=prod");
    }

    let inst_id = env_or_default("OKX_TEST_INST_ID", "ETH-USDT-SWAP");
    let side = env_or_default("OKX_TEST_SIDE", "buy");
    let size = env_or_default("OKX_TEST_ORDER_SIZE", "1");

    let api = build_simulated_api_config()?;
    api.validate().map_err(|e| anyhow::anyhow!(e))?;

    let okx = OkxOrderService;

    let last = fetch_last_price(&inst_id).await?;
    println!("last: {}", last);
    let (tp, sl) = compute_tp_sl(last, &side);
    println!("tp: {:?}, sl: {:?}", tp, sl);

    let pos_side = if side == "buy" { "long" } else { "short" };

    print!("{:?}", tp);
    print!("{:?}", sl);
    // Place order (with TP/SL attachAlgo)
    let order = place_order(&okx, &api, &inst_id, &side, size, tp, sl).await?;
    println!("order.ord_id: {}", order.ord_id);

    // Wait until position shows up
    wait_for_position(&okx, &api, &inst_id, pos_side, true).await?;

    // Move stop loss to breakeven (amend-order)
    // For test stability, use the previously fetched price as "breakeven" reference.
    let entry_price = last;

    let amender = OkxStopLossAmender::from_exchange_api_config(&api)?;
    if let Err(e) = amender
        .move_stop_loss_to_price(&inst_id, &order.ord_id, entry_price)
        .await
    {
        // 模拟盘订单可能瞬时成交/取消，导致改单返回“already filled or canceled”
        let msg = format!("{e}");
        if !msg.contains("already been filled or canceled") {
            return Err(e);
        } else {
            eprintln!("⚠️ stop-loss amend skipped: {}", msg);
        }
    }

    // Close position (market close_position)
    let mgn_mode = get_position_mgn_mode(&okx, &api, &inst_id, pos_side)
        .await?
        .unwrap_or_else(|| "isolated".to_string());
    let okx_pos_side = if side == "buy" {
        okx::dto::PositionSide::Long
    } else {
        okx::dto::PositionSide::Short
    };
    okx.close_position(&api, &inst_id, okx_pos_side, &mgn_mode)
        .await?;

    // Wait until position disappears
    wait_for_position(&okx, &api, &inst_id, pos_side, false).await?;

    Ok(())
}
