fn position_summaries(positions: &[Position]) -> Vec<Value> {
    positions
        .iter()
        .map(|position| {
            json!({
                "exchange": position.exchange.as_str(),
                "symbol": position.exchange_symbol,
                "side": position.side,
                "size": position.size,
                "entry_price": position.entry_price,
                "mark_price": position.mark_price,
                "unrealized_pnl": position.unrealized_pnl,
                "leverage": position.leverage,
                "margin_mode": position.margin_mode,
            })
        })
        .collect()
}

fn fill_summaries(fills: &[Fill]) -> Vec<Value> {
    fills
        .iter()
        .map(|fill| {
            json!({
                "exchange": fill.exchange.as_str(),
                "symbol": fill.exchange_symbol,
                "trade_id": fill.trade_id,
                "order_id": fill.order_id,
                "side": fill.side,
                "price": fill.price,
                "size": fill.size,
                "fee": fill.fee,
                "fee_asset": fill.fee_asset,
                "role": fill.role,
                "timestamp": fill.timestamp,
            })
        })
        .collect()
}

fn open_order_summaries(open_orders: &[Order]) -> Vec<Value> {
    open_orders
        .iter()
        .map(|order| {
            json!({
                "exchange": order.exchange.as_str(),
                "symbol": order.exchange_symbol,
                "order_id": order.order_id,
                "client_order_id": order.client_order_id,
                "side": order.side,
                "order_type": order.order_type,
                "size": order.size,
                "filled_size": order.filled_size,
                "status": order.status,
            })
        })
        .collect()
}

fn non_zero_position_count(positions: &[Position]) -> usize {
    positions
        .iter()
        .filter(|position| positive_decimal_text(&position.size))
        .count()
}

fn active_open_order_count(open_orders: &[Order]) -> usize {
    open_orders
        .iter()
        .filter(|order| active_open_order_status(order.status.as_deref()))
        .count()
}

fn same_exchange_symbol(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn exchange_account_snapshot_source_ref(config: &ReconciliationSnapshotCheckConfig) -> String {
    let account_hash = anonymized_account_ref(&config.buyer_email);
    let credential_ref = config
        .credential_ref
        .as_deref()
        .map(safe_source_ref_component)
        .unwrap_or_else(|| "cred_unknown".to_string());
    format!(
        "rq:acct:v1:ex={}:acct={}:cred={}:combo={}:task={}:sym={}",
        config.exchange.as_str(),
        account_hash,
        credential_ref,
        config.combo_id,
        config.task_id,
        safe_source_ref_component(&config.symbol),
    )
}

fn anonymized_account_ref(raw: &str) -> String {
    let normalized = raw.trim().to_ascii_lowercase();
    let digest = rust_quant_common::utils::function::sha256(&normalized);
    format!("email_sha256_{}", &digest[..16])
}

fn safe_source_ref_component(raw: &str) -> String {
    let component: String = raw
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .take(64)
        .collect();
    if component.is_empty() {
        "unknown".to_string()
    } else {
        component
    }
}

fn trimmed_optional(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn decimal_option(raw: Option<&str>) -> Option<f64> {
    raw.and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

fn balance_equity_usdt(balance: &Balance, wallet_balance: Option<f64>) -> Option<f64> {
    raw_number_field(
        &balance.raw,
        &["disEq", "usdtEquity", "usdt_equity", "usdValue"],
    )
    .or_else(|| {
        let asset = balance.asset.trim().to_ascii_uppercase();
        if matches!(asset.as_str(), "USDT" | "USDC" | "USD") {
            wallet_balance
        } else {
            None
        }
    })
}

fn raw_number_field(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        let raw = value.get(*key)?;
        raw.as_f64()
            .or_else(|| raw.as_str()?.trim().parse::<f64>().ok())
            .filter(|parsed| parsed.is_finite())
    })
}

fn multiply_optional(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    let value = left? * right?;
    value
        .is_finite()
        .then_some((value * 100_000_000.0).round() / 100_000_000.0)
}

fn timestamp_millis_to_naive_string(timestamp_millis: u64) -> Option<String> {
    let timestamp_millis = i64::try_from(timestamp_millis).ok()?;
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_millis)
        .map(|value| web_naive_datetime_string(value.naive_utc()))
}

fn web_naive_datetime_string(value: chrono::NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn normalized_fill_side(fill: &Fill) -> Option<String> {
    fill.side
        .as_deref()
        .map(str::trim)
        .filter(|side| !side.is_empty())
        .map(str::to_ascii_lowercase)
}

fn positive_decimal_option(value: Option<&str>) -> bool {
    value.is_some_and(positive_decimal_text)
}

fn decimal_texts_equal(left: Option<&str>, right: Option<&str>) -> bool {
    let Some(left) = left else {
        return false;
    };
    let Some(right) = right else {
        return false;
    };
    let Ok(left) = left.trim().parse::<f64>() else {
        return false;
    };
    let Ok(right) = right.trim().parse::<f64>() else {
        return false;
    };
    left.is_finite() && right.is_finite() && (left - right).abs() < 1e-12
}

fn positive_decimal_text(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|parsed| parsed.is_finite() && parsed.abs() > 0.0)
}

fn active_open_order_status(status: Option<&str>) -> bool {
    let normalized = status.unwrap_or_default().trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "canceled" | "cancelled" | "filled" | "closed" | "rejected" | "expired"
    )
}
