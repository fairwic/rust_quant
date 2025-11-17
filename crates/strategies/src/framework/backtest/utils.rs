use rust_quant_market::models::CandlesEntity;
use crate::CandleItem;

/// 解析价格
pub fn parse_price(candle: &CandlesEntity) -> f64 {
    candle.c.parse::<f64>().unwrap_or_else(|e| {
        tracing::error!("Failed to parse price: {}", e);
        0.0
    })
}

/// 计算盈亏
pub fn calculate_profit_loss(is_long: bool, position: f64, entry_price: f64, exit_price: f64) -> f64 {
    if is_long {
        position * (exit_price - entry_price)
    } else {
        position * (entry_price - exit_price)
    }
}

/// 将CandlesEntity转换为CandleItem
pub fn parse_candle_to_data_item(candle: &CandlesEntity) -> CandleItem {
    CandleItem::builder()
        .c(candle.c.parse::<f64>().unwrap())
        .v(candle.vol_ccy.parse::<f64>().unwrap())
        .h(candle.h.parse::<f64>().unwrap())
        .l(candle.l.parse::<f64>().unwrap())
        .o(candle.o.parse::<f64>().unwrap())
        .confirm(candle.confirm.parse::<i32>().unwrap())
        .ts(candle.ts)
        .build()
        .unwrap()
}

/// 计算胜率
pub fn calculate_win_rate(wins: i64, losses: i64) -> f64 {
    if wins + losses > 0 {
        wins as f64 / (wins + losses) as f64
    } else {
        0.0
    }
}

