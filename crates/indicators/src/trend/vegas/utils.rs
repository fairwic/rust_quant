use rust_quant_common::utils::fibonacci::{
    FIBONACCI_ZERO_POINT_THREE_EIGHT_TWO, FIBONACCI_ZERO_POINT_TWO_THREE_SIX,
};
use rust_quant_common::CandleItem;
use tracing::debug;

/// 计算当前K线价格的振幅
pub fn calculate_k_line_amplitude(data_items: &[CandleItem]) -> f64 {
    let mut amplitude = 0.0;
    if let Some(last_item) = data_items.last() {
        // 计算最高价和最低价之间的差异
        let high = last_item.h();
        let low = last_item.l();
        // 使用开盘价作为基准计算振幅百分比
        let open = last_item.o();
        if open != 0.0 {
            // 振幅计算: (最高价 - 最低价) / 开盘价 * 100
            amplitude = (high - low) / open * 100.0;
        }
    }
    amplitude
}

/// 计算最优开仓价格
pub fn calculate_best_open_price(
    data_items: &[CandleItem],
    should_buy: bool,
    should_sell: bool,
) -> Option<f64> {
    let last_data_item = data_items.last()?;
    let amplitude = calculate_k_line_amplitude(data_items);

    if amplitude <= 1.2 {
        debug!("k线振幅小于1.5个点，不计算最优开仓价格");
        return None;
    }

    let high_price = last_data_item.h();
    let low_price = last_data_item.l();
    let diff = high_price - low_price;

    if should_sell {
        // 如果k线是下跌，且跌幅较大，且没有利空消息，则使用最优开仓价格
        // (当前k线最高价格-当前k线最低价格)的38.2%作为最优开仓价格
        Some(low_price + diff * FIBONACCI_ZERO_POINT_THREE_EIGHT_TWO)
    } else if should_buy {
        // 如果k线是上涨，且涨幅较大，且没有利好消息，则使用最优开仓价格
        // (当前k线最高价格-当前k线最低价格)的23.6%作为最优开仓价格
        Some(high_price - (diff * FIBONACCI_ZERO_POINT_THREE_EIGHT_TWO))
    } else {
        None
    }
}

/// 计算最佳止损价格
pub fn calculate_best_stop_loss_price(
    last_data_item: &CandleItem,
    should_buy: bool,
    should_sell: bool,
) -> Option<f64> {
    if should_buy {
        // let amplitude = last_data_item.h() - last_data_item.l();
        // Some(last_data_item.l() + (amplitude * FIBONACCI_ZERO_POINT_TWO_THREE_SIX))
        Some(last_data_item.l())
    } else if should_sell {
        // let amplitude = last_data_item.h() - last_data_item.l();
        // Some(last_data_item.h() - (amplitude * FIBONACCI_ZERO_POINT_TWO_THREE_SIX))
        Some(last_data_item.h())
    } else {
        None
    }
}

/// 计算最优止盈价格
pub fn calculate_best_take_profit_price(
    last_data_item: &CandleItem,
    should_buy: bool,
    should_sell: bool,
) -> Option<f64> {
    if should_buy {
        let amplitude = last_data_item.c() - last_data_item.l();
        Some(last_data_item.c() + (amplitude * 4.0))
    } else if should_sell {
        let amplitude = last_data_item.c() - last_data_item.l();
        Some(last_data_item.c() - (amplitude * 4.0))
    } else {
        None
    }
}

/// 检查关键价位卖出信号
pub fn check_key_price_level_sell(
    current_price: f64,
    volume_is_increasing: bool,
) -> Option<String> {
    // 定义价位级别和对应的提前预警距离
    const PRICE_LEVELS: [(f64, f64, f64, &str); 8] = [
        // (价位区间, 提前预警百分比, 建议回撤百分比, 级别描述)
        (10000.0, 0.02, 0.015, "万元"), // 万元级别
        (1000.0, 0.015, 0.01, "千元"),  // 千元级别
        (100.0, 0.01, 0.008, "百元"),   // 百元级别
        (10.0, 0.008, 0.005, "十元"),   // 十元级别
        (1.0, 0.005, 0.003, "元"),      // 1元级别
        (0.1, 0.003, 0.002, "角"),      // 0.1元级别
        (0.01, 0.002, 0.001, "分"),     // 0.01元级别
        (0.001, 0.001, 0.0005, "厘"),   // 0.001元级别
    ];

    // 从大到小遍历找到第一个小于等于当前价格的级别
    let (interval, alert_percent, pullback_percent, level_name) = PRICE_LEVELS
        .iter()
        .find(|&&(level, _, _, _)| current_price >= level)
        .unwrap_or(&(0.001, 0.001, 0.0005, "微"));

    // 计算下一个关键价位
    let price_unit = if *interval >= 1.0 {
        *interval / 10.0 // 对于大于1元的价格，使用十分之一作为单位
    } else {
        *interval // 对于小于1元的价格，使用当前区间作为单位
    };

    let next_key_level = if *interval >= 1.0 {
        let magnitude = 10f64.powi((*interval as f64).log10().floor() as i32);
        (*interval / magnitude).floor() * magnitude
    } else {
        let magnitude = 10f64.powi((1.0 / *interval as f64).log10().ceil() as i32);
        (*interval * magnitude).floor() / magnitude
    };

    let distance_to_key = next_key_level - current_price;
    let alert_distance = next_key_level * alert_percent;

    println!(
        "价位分析 - 当前价格: {:.4}, 下一关键位: {:.4}, 距离: {:.4}, 预警距离: {:.4} [{}级别]",
        current_price, next_key_level, distance_to_key, alert_distance, level_name
    );

    // 如果接近关键价位且成交量增加，生成卖出信号
    if distance_to_key > 0.0 && distance_to_key < alert_distance && volume_is_increasing {
        // 动态计算建议卖出价格
        let suggested_sell_price = if *interval >= 1.0 {
            // 大额价格使用百分比回撤
            next_key_level * (1.0 - pullback_percent)
        } else {
            // 小额价格使用固定点位回撤
            next_key_level - (price_unit * pullback_percent)
        };

        // 根据价格级别确定信号类型
        let signal_type = if *interval >= 100.0 {
            "重要"
        } else {
            "普通"
        };

        println!("价位分析详情:");
        println!("  价格级别: {} (区间: {:.4})", level_name, interval);
        println!("  预警比例: {:.2}%", alert_percent * 100.0);
        println!("  建议回撤: {:.2}%", pullback_percent * 100.0);
        println!("  建议卖价: {:.4}", suggested_sell_price);

        let format_str = if *interval >= 1.0 {
            format!(
                "{}价位卖出信号: 当前价格({:.2})接近{}级别关键位({:.2})，建议在{:.2}卖出 [回撤{:.1}%]",
                signal_type, current_price, level_name, next_key_level, suggested_sell_price,
                pullback_percent * 100.0
            )
        } else {
            format!(
                "{}价位卖出信号: 当前价格({:.4})接近{}级别关键位({:.4})，建议在{:.4}卖出 [回撤{:.2}%]",
                signal_type, current_price, level_name, next_key_level, suggested_sell_price,
                pullback_percent * 100.0
            )
        };

        return Some(format_str);
    }

    None
}
