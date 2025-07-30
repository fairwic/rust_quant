use crate::trading;
use crate::trading::indicator::squeeze_momentum::calculator::SqueezeCalculator;
use crate::trading::indicator::squeeze_momentum::squeeze_config::SqueezeConfig;
use crate::trading::model::entity::candles::enums::SelectTime;
use crate::trading::strategy::strategy_common::SignalResult;

/// 返回线性回归的结果 (f64)。
pub fn calculate_linreg(source: &[f64], length: usize, offset: usize) -> Option<f64> {
    if source.len() < length || length == 0 {
        return None; // 数据不足或长度为零时返回 None
    }

    // 准备计算
    let start_idx = source.len() - length; // 回归的起始索引
    let y = &source[start_idx..]; // 截取需要的子序列

    // 求和
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_x2 = 0.0;
    let mut sum_xy = 0.0;

    for i in 0..length {
        let x = i as f64;
        let yi = y[i];
        sum_x += x;
        sum_y += yi;
        sum_x2 += x * x;
        sum_xy += x * yi;
    }

    // 计算斜率 (slope) 和截距 (intercept)
    let n = length as f64;
    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
    let intercept = (sum_y - slope * sum_x) / n;

    // 计算 linreg 结果
    let result = intercept + slope * ((length - 1 - offset) as f64);
    Some(result)
}


pub async fn get_last_squeeze_single(
    config: SqueezeConfig,
    inst_id: &str,
    period: &str,
    select_time: Option<SelectTime>,
) -> anyhow::Result<SignalResult> {
    let min_length = config.bb_length.max(config.kc_length);
    let candles = trading::task::basic::get_candle_data_confirm(inst_id, period, min_length * 2, select_time).await?;
    if candles.len() < min_length {
        return Err(anyhow::anyhow!("Insufficient data"));
    }
    //组装数据
    //初始化配置类
    let mut calculator = SqueezeCalculator::new(config);
    //计算
    let mut result = calculator.get_trade_signal(&candles);
    // result.timestamp = candles.last().unwrap().ts;

    Ok(result)
}
