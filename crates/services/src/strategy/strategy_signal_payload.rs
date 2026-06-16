use anyhow::{anyhow, Result};
use chrono::SecondsFormat;
use rust_quant_strategies::framework::risk::{StopLossCalculator, StopLossSide};
use rust_quant_strategies::strategy_common::SignalResult;
use serde_json::{json, Value};

use crate::rust_quan_web::StrategySignalSubmitRequest;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StrategySignalPayloadBuildOptions {
    pub source_signal_type: String,
    pub external_id_override: Option<String>,
    pub payload_overlay: Option<Value>,
}

impl Default for StrategySignalPayloadBuildOptions {
    fn default() -> Self {
        Self {
            source_signal_type: "technical_strategy".to_string(),
            external_id_override: None,
            payload_overlay: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_strategy_signal_submit_request(
    inst_id: &str,
    period: &str,
    signal: &SignalResult,
    risk_config: &rust_quant_domain::BasicRiskConfig,
    config_id: i64,
    strategy_type: &str,
    exchange: Option<&str>,
    side: &str,
    pos_side: &str,
    client_order_id: &str,
    options: StrategySignalPayloadBuildOptions,
) -> Result<StrategySignalSubmitRequest> {
    let direction = match pos_side {
        "long" | "short" => pos_side.to_string(),
        other => {
            return Err(anyhow!(
                "unsupported strategy signal position side: {}",
                other
            ))
        }
    };
    let selected_stop_loss = select_strategy_signal_stop_loss(side, pos_side, signal, risk_config)?;
    let entry_price = signal.open_price;
    let generated_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(signal.ts)
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true));
    let strategy_key = format!("{strategy_type}:{inst_id}:{period}:{config_id}");

    let mut payload_json = json!({
        "source": "rust_quant",
        "source_signal_type": options.source_signal_type,
        "config_id": config_id,
        "strategy_type": strategy_type,
        "strategy_key": &strategy_key,
        "period": period,
        "symbol": inst_id,
        "exchange": exchange.map(str::to_ascii_lowercase),
        "side": side,
        "position_side": pos_side,
        "trade_side": "open",
        "order_type": "market",
        "client_order_id": client_order_id,
        "risk_plan": {
            "entry_price": entry_price,
            "selected_stop_loss_price": selected_stop_loss,
            "direction": pos_side,
            "protective_stop_loss_required": true,
        },
        "signal": signal,
    });
    if let Some(overlay) = options.payload_overlay {
        merge_json_object(&mut payload_json, overlay);
    }

    let smoke_external_id_suffix = std::env::var("RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX").ok();
    let external_id = options.external_id_override.unwrap_or_else(|| {
        build_strategy_signal_external_id(
            strategy_type,
            config_id,
            inst_id,
            period,
            signal.ts,
            smoke_external_id_suffix.as_deref(),
        )
    });

    Ok(StrategySignalSubmitRequest {
        source: "rust_quant".to_string(),
        external_id,
        strategy_slug: strategy_type.to_string(),
        strategy_key,
        symbol: inst_id.to_string(),
        signal_type: "entry".to_string(),
        direction,
        title: format!(
            "{} {} signal {} {}",
            title_case_strategy(strategy_type),
            pos_side,
            inst_id,
            period
        ),
        summary: Some(format!(
            "rust_quant strategy {} generated {} entry signal at price {}",
            strategy_type, pos_side, signal.open_price
        )),
        confidence: None,
        payload_json: payload_json.to_string(),
        generated_at,
    })
}

pub(crate) fn build_strategy_signal_external_id(
    strategy_type: &str,
    config_id: i64,
    inst_id: &str,
    period: &str,
    signal_ts: i64,
    smoke_suffix: Option<&str>,
) -> String {
    let base = format!(
        "rust_quant:{}:{}:{}:{}:{}",
        strategy_type, config_id, inst_id, period, signal_ts
    );
    match smoke_suffix
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(suffix) => format!("{base}:{suffix}"),
        None => base,
    }
}

fn title_case_strategy(strategy_type: &str) -> String {
    let mut chars = strategy_type.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Strategy".to_string(),
    }
}

fn select_strategy_signal_stop_loss(
    side: &str,
    pos_side: &str,
    signal: &SignalResult,
    risk_config: &rust_quant_domain::BasicRiskConfig,
) -> Result<f64> {
    let entry_price = signal.open_price;
    if !entry_price.is_finite() || entry_price <= 0.0 {
        return Err(anyhow!("策略信号开仓价无效: {}", entry_price));
    }

    let stop_side = match side {
        "buy" => StopLossSide::Long,
        "sell" => StopLossSide::Short,
        other => return Err(anyhow!("unsupported strategy signal side: {}", other)),
    };
    let stop_candidates = build_stop_loss_candidates(side, signal, risk_config);
    let selected_stop_loss = StopLossCalculator::select(stop_side, entry_price, &stop_candidates)
        .ok_or_else(|| anyhow!("无有效止损价"))?;

    if pos_side == "short" && entry_price > selected_stop_loss {
        return Err(anyhow!(
            "做空开仓价 > 止损价，不提交Web信号: entry={}, stop_loss={}",
            entry_price,
            selected_stop_loss
        ));
    }
    if pos_side == "long" && entry_price < selected_stop_loss {
        return Err(anyhow!(
            "做多开仓价 < 止损价，不提交Web信号: entry={}, stop_loss={}",
            entry_price,
            selected_stop_loss
        ));
    }

    Ok(selected_stop_loss)
}

fn build_stop_loss_candidates(
    side: &str,
    signal: &SignalResult,
    risk_config: &rust_quant_domain::BasicRiskConfig,
) -> Vec<f64> {
    let entry_price = signal.open_price;
    let max_loss_percent = risk_config.max_loss_percent;
    let max_loss_stop = if side == "sell" {
        entry_price * (1.0 + max_loss_percent)
    } else {
        entry_price * (1.0 - max_loss_percent)
    };

    let mut candidates: Vec<f64> = vec![max_loss_stop];

    if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
        if let Some(px) = signal.signal_kline_stop_loss_price {
            candidates.push(px);
        }
    }

    candidates
}

fn merge_json_object(target: &mut Value, overlay: Value) {
    match (target, overlay) {
        (Value::Object(target_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match target_map.get_mut(&key) {
                    Some(target_value) => merge_json_object(target_value, overlay_value),
                    None => {
                        target_map.insert(key, overlay_value);
                    }
                }
            }
        }
        (target_value, overlay_value) => {
            *target_value = overlay_value;
        }
    }
}
