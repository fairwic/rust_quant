use crate::CandleItem;

/// 解析价格
pub fn parse_price(candle: &CandleItem) -> f64 {
    candle.c
}

/// 计算盈亏
pub fn calculate_profit_loss(is_long: bool, position: f64, entry_price: f64, exit_price: f64) -> f64 {
    if is_long {
        position * (exit_price - entry_price)
    } else {
        position * (entry_price - exit_price)
    }
}

/// CandleItem 在策略层即为标准输入，保持接口名仅为向后兼容
pub fn parse_candle_to_data_item(candle: &CandleItem) -> CandleItem {
    candle.clone()
}

/// 计算胜率
pub fn calculate_win_rate(wins: i64, losses: i64) -> f64 {
    if wins + losses > 0 {
        wins as f64 / (wins + losses) as f64
    } else {
        0.0
    }
}

