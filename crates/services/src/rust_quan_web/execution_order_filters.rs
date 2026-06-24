use super::execution_protection::ProtectiveDirection;
use anyhow::{anyhow, Result};
use crypto_exc_all::ExchangeId;
use rust_decimal::{Decimal, RoundingStrategy};
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ExchangeOrderFilters {
    /// 数量数值。
    pub(super) min_qty: Option<Decimal>,
    /// 数量数值。
    pub(super) max_qty: Option<Decimal>,
    /// 数量数值。
    pub(super) step_size: Option<Decimal>,
    /// 最小名义金额；为空时使用交易所默认值。
    pub(super) min_notional: Option<Decimal>,
    /// 数量精度；为空时使用交易所默认值。
    pub(super) quantity_precision: Option<u32>,
    /// 数量数值。
    pub(super) tick_size: Option<Decimal>,
    /// 价格精度；为空时使用交易所默认值。
    pub(super) price_precision: Option<u32>,
    /// contract值；为空时表示该条件不启用。
    pub(super) contract_value: Option<Decimal>,
    /// contract值currency；为空时表示该条件不启用。
    pub(super) contract_value_currency: Option<String>,
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
pub(super) async fn load_exchange_order_filters(
    exchange: ExchangeId,
    symbol: &str,
) -> Result<Option<ExchangeOrderFilters>> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
        .map_err(|_| anyhow!("QUANT_CORE_DATABASE_URL is required for live order filter checks"))?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    let row = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i32>,
            Option<String>,
            Option<i32>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT
            min_qty,
            max_qty,
            step_size,
            min_notional,
            quantity_precision,
            tick_size,
            price_precision,
            raw_payload #>> '{ctVal}',
            raw_payload #>> '{ctValCcy}'
        FROM exchange_symbols
        WHERE exchange = $1
          AND normalized_symbol = $2
          AND lower(status) IN ('trading', 'live')
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(exchange.as_str())
    .bind(symbol)
    .fetch_optional(&pool)
    .await?;
    pool.close().await;
    row.map(
        |(
            min_qty,
            max_qty,
            step_size,
            min_notional,
            quantity_precision,
            tick_size,
            price_precision,
            contract_value,
            contract_value_currency,
        )| {
            Ok(ExchangeOrderFilters {
                min_qty: parse_optional_decimal(min_qty.as_deref(), "min_qty")?,
                max_qty: parse_optional_decimal(max_qty.as_deref(), "max_qty")?,
                step_size: parse_optional_decimal(step_size.as_deref(), "step_size")?,
                min_notional: parse_optional_decimal(min_notional.as_deref(), "min_notional")?,
                quantity_precision: quantity_precision.and_then(|value| u32::try_from(value).ok()),
                tick_size: parse_optional_decimal(tick_size.as_deref(), "tick_size")?,
                price_precision: price_precision.and_then(|value| u32::try_from(value).ok()),
                contract_value: parse_optional_decimal(
                    contract_value.as_deref(),
                    "contract_value",
                )?,
                contract_value_currency: contract_value_currency
                    .map(|value| value.trim().to_ascii_uppercase())
                    .filter(|value| !value.is_empty()),
            })
        },
    )
    .transpose()
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
fn parse_optional_decimal(raw: Option<&str>, label: &str) -> Result<Option<Decimal>> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<Decimal>()
                .map_err(|error| anyhow!("invalid {label} exchange filter {value}: {error}"))
        })
        .transpose()
}
/// 解析输入参数并收敛为 Web 商业、会员和执行准备度 可使用的结构化值。
pub(super) fn parse_positive_decimal(raw: &str, label: &str) -> Result<Decimal> {
    let value = raw
        .trim()
        .parse::<Decimal>()
        .map_err(|error| anyhow!("invalid {label} {raw}: {error}"))?;
    if value <= Decimal::ZERO {
        return Err(anyhow!("{label} must be positive"));
    }
    Ok(value)
}
/// 提供小数fromf64的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn decimal_from_f64(raw: f64) -> Result<Decimal> {
    if !raw.is_finite() || raw <= 0.0 {
        return Err(anyhow!("price must be a positive finite number"));
    }
    format!("{raw:.12}")
        .parse::<Decimal>()
        .map_err(|error| anyhow!("invalid decimal price {raw}: {error}"))
}
/// 提供floortostep的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn floor_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    (value / step).floor() * step
}
/// 提供ceiltostep的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn ceil_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    let floored = floor_to_step(value, step);
    if floored == value {
        floored
    } else {
        floored + step
    }
}
/// 提供notionalpersizeunit的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn notional_per_size_unit(last_price: Decimal, filters: &ExchangeOrderFilters) -> Decimal {
    let Some(contract_value) = filters
        .contract_value
        .filter(|value| *value > Decimal::ZERO)
    else {
        return last_price;
    };
    match filters.contract_value_currency.as_deref() {
        Some("USDT" | "USD") => contract_value,
        Some(_) => contract_value * last_price,
        None => last_price,
    }
}
/// 按交易所合约规则计算订单名义金额，用于校验最终订单没有超过 Web 预留预算。
pub(super) fn order_notional_usdt(
    size: Decimal,
    last_price: Decimal,
    filters: &ExchangeOrderFilters,
) -> Result<f64> {
    let notional = size * notional_per_size_unit(last_price, filters);
    let value = notional
        .normalize()
        .to_string()
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid order notional {notional}: {error}"))?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(anyhow!("order notional must be finite and non-negative"))
    }
}
/// 提供量化订单size的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn quantize_order_size(
    requested_size: Decimal,
    last_price: Decimal,
    filters: &ExchangeOrderFilters,
    enforce_min_notional: bool,
) -> Result<Decimal> {
    if requested_size <= Decimal::ZERO {
        return Err(anyhow!("order size must be positive"));
    }
    let mut size = requested_size;
    if let Some(step) = filters.step_size.filter(|value| *value > Decimal::ZERO) {
        size = floor_to_step(size, step);
    } else if let Some(precision) = filters.quantity_precision {
        size = size.round_dp_with_strategy(precision, RoundingStrategy::ToZero);
    }
    if size <= Decimal::ZERO {
        return Err(anyhow!(
            "order size is below exchange step size after quantization"
        ));
    }
    if let Some(min_qty) = filters.min_qty {
        if size < min_qty {
            return Err(anyhow!(
                "order size {} is below exchange min_qty {}",
                format_order_size_decimal(size, filters),
                min_qty
            ));
        }
    }
    if let Some(max_qty) = filters.max_qty {
        if max_qty > Decimal::ZERO && size > max_qty {
            return Err(anyhow!(
                "order size {} is above exchange max_qty {}",
                format_order_size_decimal(size, filters),
                max_qty
            ));
        }
    }
    if enforce_min_notional {
        if let Some(min_notional) = filters.min_notional {
            let notional = size * notional_per_size_unit(last_price, filters);
            if min_notional > Decimal::ZERO && notional < min_notional {
                return Err(anyhow!(
                    "order notional {} is below exchange min_notional {} after size quantization",
                    notional,
                    min_notional
                ));
            }
        }
    }
    Ok(size)
}
/// 计算最小订单size，并把公式和边界条件集中在Web 商业链路内部。
pub(super) fn minimum_order_size(
    last_price: Decimal,
    filters: &ExchangeOrderFilters,
    enforce_min_notional: bool,
) -> Result<Decimal> {
    if last_price <= Decimal::ZERO {
        return Err(anyhow!(
            "last_price must be positive for minimum order size"
        ));
    }
    let mut size = filters.min_qty.unwrap_or(Decimal::ZERO);
    if enforce_min_notional {
        if let Some(min_notional) = filters.min_notional.filter(|value| *value > Decimal::ZERO) {
            size = size.max(min_notional / notional_per_size_unit(last_price, filters));
        }
    }
    if let Some(step) = filters.step_size.filter(|value| *value > Decimal::ZERO) {
        size = ceil_to_step(size, step);
    } else if let Some(precision) = filters.quantity_precision {
        size = ceil_to_step(size, Decimal::new(1, precision));
    }
    if size <= Decimal::ZERO {
        return Err(anyhow!(
            "exchange filters do not define a positive minimum order size"
        ));
    }
    quantize_order_size(size, last_price, filters, enforce_min_notional)
}
/// 计算交易所允许的最小名义金额，供下单前全局风险预留判断是否值得占用预算。
pub(super) fn minimum_order_notional_usdt(
    last_price: Decimal,
    filters: &ExchangeOrderFilters,
    enforce_min_notional: bool,
) -> Result<Option<f64>> {
    if filters.min_qty.is_none() && !(enforce_min_notional && filters.min_notional.is_some()) {
        return Ok(None);
    }
    let size = minimum_order_size(last_price, filters, enforce_min_notional)?;
    let notional = size * notional_per_size_unit(last_price, filters);
    let value = notional
        .normalize()
        .to_string()
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid minimum order notional {notional}: {error}"))?;
    if value.is_finite() && value > 0.0 {
        Ok(Some(value))
    } else {
        Ok(None)
    }
}
/// 生成 Web 商业、会员和执行准备度 需要的派生数据，供后续执行、展示或审计使用。
pub(super) fn format_order_size_decimal(size: Decimal, filters: &ExchangeOrderFilters) -> String {
    let precision = filters
        .quantity_precision
        .or_else(|| filters.step_size.map(|step| step.scale()));
    let normalized = match precision {
        Some(precision) => size.round_dp_with_strategy(precision, RoundingStrategy::ToZero),
        None => size,
    }
    .normalize();
    normalized.to_string()
}
/// 提供量化protective止损价格的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub(super) fn quantize_protective_stop_price(
    price: f64,
    direction: ProtectiveDirection,
    filters: &ExchangeOrderFilters,
) -> Result<Decimal> {
    let price = decimal_from_f64(price)?;
    let step = filters
        .tick_size
        .filter(|value| *value > Decimal::ZERO)
        .or_else(|| {
            filters
                .price_precision
                .map(|precision| Decimal::new(1, precision))
        });
    let Some(step) = step else {
        return Ok(price);
    };
    let normalized = match direction {
        ProtectiveDirection::Long => floor_to_step(price, step),
        ProtectiveDirection::Short => ceil_to_step(price, step),
    };
    if normalized <= Decimal::ZERO {
        return Err(anyhow!(
            "protective stop price is below exchange tick size after quantization"
        ));
    }
    Ok(normalized)
}
/// 按交易方向把开仓 limit 价格归一到交易所 tick，避免扩大用户给出的最差成交价。
pub(super) fn quantize_limit_order_price(
    raw: &str,
    side: crypto_exc_all::OrderSide,
    filters: &ExchangeOrderFilters,
) -> Result<Decimal> {
    let price = parse_positive_decimal(raw, "limit price")?;
    let step = filters
        .tick_size
        .filter(|value| *value > Decimal::ZERO)
        .or_else(|| {
            filters
                .price_precision
                .map(|precision| Decimal::new(1, precision))
        });
    let Some(step) = step else {
        return Ok(price);
    };
    let normalized = match side {
        crypto_exc_all::OrderSide::Buy => floor_to_step(price, step),
        crypto_exc_all::OrderSide::Sell => ceil_to_step(price, step),
    };
    if normalized <= Decimal::ZERO {
        return Err(anyhow!(
            "limit price is below exchange tick size after quantization"
        ));
    }
    Ok(normalized)
}
/// 生成 Web 商业、会员和执行准备度 需要的派生数据，供后续执行、展示或审计使用。
pub(super) fn format_protective_stop_price_decimal(
    price: Decimal,
    filters: &ExchangeOrderFilters,
) -> String {
    let precision = filters
        .price_precision
        .or_else(|| filters.tick_size.map(|step| step.scale()));
    let normalized = match precision {
        Some(precision) => price.round_dp_with_strategy(precision, RoundingStrategy::ToZero),
        None => price,
    }
    .normalize();
    normalized.to_string()
}
/// 生成普通订单价格字符串，复用交易所价格精度和 tick 规则。
pub(super) fn format_order_price_decimal(price: Decimal, filters: &ExchangeOrderFilters) -> String {
    format_protective_stop_price_decimal(price, filters)
}
