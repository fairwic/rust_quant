use dotenv::dotenv;
use ndarray::{s, Array1};
use rust_quant::app_config::db::init_db;
use rust_quant::time_util;
use rust_quant::trading;
use rust_quant::trading::model::market::candles::CandlesEntity;
use std::f32;
use ta::indicators::SimpleMovingAverage;
use ta::{Close, Next};
// 方案2：使用实现了 Close trait 的数据结构
use ta::DataItem;
// 定义一个通用的类型别名，可以在项目中切换浮动点类型
// 可以在此处替换为 `f32` 或 `f64` 来决定浮点精度

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SqueezeState {
    SqueezeOn,  // 压缩状态开启
    SqueezeOff, // 压缩状态关闭
    NoSqueeze,  // 没有压缩状态
}

#[derive(Debug)]
pub struct SqueezeMomentumIndicator {
    pub upper_bb: f64,               // 布林带上轨
    pub lower_bb: f64,               // 布林带下轨
    pub upper_kc: f64,               // 凯尔特纳通道上轨
    pub lower_kc: f64,               // 凯尔特纳通道下轨
    pub squeeze_state: SqueezeState, // 压缩状态
    pub momentum: f64,               // 动量值
    pub basis: f64,                  // 布林带基础（中轨）
    pub dev: f64,                    // 布林带标准差
}
// 为 f64 实现 Close trait
// 使用 ta::indicators::SimpleMovingAverage 来计算 SMA
fn sma(data: &Array1<f64>, length: usize) -> Array1<f64> {
    let vec_data: Vec<DataItem> = data
        .iter()
        .map(|&x| {
            DataItem::builder()
                .close(x)
                .open(x)
                .high(x)
                .low(x)
                .volume(0.0)
                .build()
                .unwrap()
        })
        .collect();
    let mut sma_indicator = SimpleMovingAverage::new(length).unwrap();
    let result = vec_data
        .iter()
        .map(|x| sma_indicator.next(x))
        .collect::<Vec<f64>>();
    Array1::from(result)
}

// 计算标准差
fn stdev(data: &Array1<f64>, length: usize) -> Array1<f64> {
    let sma_data = sma(data, length);
    let mut result = Array1::zeros(data.len());
    for i in length..data.len() {
        let variance: f64 = data
            .slice(s![i - length..i])
            .iter()
            .map(|&x| (x - sma_data[i]).powi(2))
            .sum::<f64>()
            / length as f64;
        result[i] = variance.sqrt();
    }
    result
}

// 计算线性回归
fn linreg(data: &Array1<f64>, length: usize, offset: i32) -> Array1<f64> {
    let mut result = Array1::zeros(data.len());

    for i in length..data.len() {
        let window = data.slice(s![i - length..i]);
        let x: Vec<f64> = (0..length).map(|i| i as f64).collect();
        let y: Vec<f64> = window.iter().copied().collect();

        let sum_x: f64 = x.iter().sum();
        let sum_y: f64 = y.iter().sum();
        let sum_xx: f64 = x.iter().map(|&xi| xi * xi).sum();
        let sum_xy: f64 = x.iter().zip(y.iter()).map(|(&xi, &yi)| xi * yi).sum();

        let slope =
            (length as f64 * sum_xy - sum_x * sum_y) / (length as f64 * sum_xx - sum_x.powi(2));
        let intercept = (sum_y - slope * sum_x) / length as f64;

        // 应用偏移
        let regression_value = intercept + slope * (length as f64 - 1.0 - offset as f64);

        result[i] = regression_value;
    }

    result
}

// 判断是否满足 Squeeze 状态
fn check_squeeze(
    lower_bb: &Array1<f64>,
    upper_bb: &Array1<f64>,
    lower_kc: &Array1<f64>,
    upper_kc: &Array1<f64>,
) -> Vec<SqueezeState> {
    let mut squeeze_states = Vec::new();
    for i in 0..lower_bb.len() {
        if lower_bb[i] > lower_kc[i] && upper_bb[i] < upper_kc[i] {
            squeeze_states.push(SqueezeState::SqueezeOn); // 压缩状态开启
        } else if lower_bb[i] < lower_kc[i] && upper_bb[i] > upper_kc[i] {
            squeeze_states.push(SqueezeState::SqueezeOff); // 压缩状态关闭
        } else {
            squeeze_states.push(SqueezeState::NoSqueeze); // 无压缩状态
        }
    }
    squeeze_states
}

// 将 CandlesEntity 转换为 PriceType 类型的价格数据，并返回时间戳
fn to_price(candle: &CandlesEntity) -> (f64, f64, f64, f64, i64) {
    (
        candle.o.parse::<f64>().unwrap_or(0.0),
        candle.h.parse::<f64>().unwrap_or(0.0),
        candle.l.parse::<f64>().unwrap_or(0.0),
        candle.c.parse::<f64>().unwrap_or(0.0),
        candle.ts, // 返回时间戳
    )
}

#[tokio::test]
async fn test_squeen_strategy() {
    // 假设的 CandlesEntity 数据 (OHLC 数据)
    dotenv().ok();
    init_db().await;
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";

    // 获取 K 线数据
    let mysql_candles = trading::task::get_candle_data(inst_id, time).await.unwrap();

    // 转换 CandlesEntity 为 PriceType 型价格数据，并保留时间戳
    let prices: Vec<(f64, f64, f64, f64, i64)> =
        mysql_candles.iter().map(|c| to_price(c)).collect();
    let close_prices: Vec<f64> = prices.iter().map(|(_, _, _, c, _)| *c).collect();
    let high_prices: Vec<f64> = prices.iter().map(|(_, h, _, _, _)| *h).collect();
    let low_prices: Vec<f64> = prices.iter().map(|(_, _, l, _, _)| *l).collect();
    let timestamps: Vec<i64> = prices.iter().map(|(_, _, _, _, ts)| *ts).collect(); // 获取时间戳

    // 使用 ndarray 存储数据
    let close = Array1::from(close_prices);
    let high = Array1::from(high_prices);
    let low = Array1::from(low_prices);

    println!("{:?}", close);
    // 计算布林带 (BB)
    let bb_length = 20;
    let bb_dev = 2.0;
    let basis = sma(&close, bb_length);
    let dev = stdev(&close, bb_length) * bb_dev;
    let upper_bb = &basis + &dev;
    let lower_bb = &basis - &dev;

    // 计算凯尔特纳通道 (KC)
    let kc_length = 20;
    let kc_mult = 1.5;
    let ma = sma(&close, kc_length);
    let range = high - low;
    let range_ma = sma(&range, kc_length);
    let upper_kc = &ma + &range_ma * kc_mult;
    let lower_kc = &ma - &range_ma * kc_mult;

    // 判断 Squeeze 状态
    let squeeze_states = check_squeeze(&lower_bb, &upper_bb, &lower_kc, &upper_kc);

    // 输出每个时间点的 Squeeze 状态，动量值，以及 K 线时间戳
    for (i, &state) in squeeze_states.iter().enumerate() {
        let momentum = linreg(&close, kc_length, 0)[i]; // 计算动量，偏移为 0
        let color = match state {
            SqueezeState::SqueezeOn => "Black",
            SqueezeState::SqueezeOff => "Yellow",
            SqueezeState::NoSqueeze => "Blue",
        };

        // 获取 basis 和 dev
        let current_basis = basis[i];
        let current_dev = dev[i];

        // 将时间戳转换为可读的日期时间格式
        let time_str = time_util::mill_time_to_datetime_shanghai(timestamps[i]).unwrap();

        // 打印输出，包含时间戳、Squeeze 状态、动量值、basis、dev等信息
        println!(
            "Time: {}, Squeeze State: {:?}, Momentum: {:.2}, Upper BB: {:.2}, Lower BB: {:.2}, Upper KC: {:.2}, Lower KC: {:.2}, Basis: {:.2}, Dev: {:.2}, Color: {}",
            time_str, state, momentum, upper_bb[i], lower_bb[i], upper_kc[i], lower_kc[i], current_basis, current_dev, color
        );
    }
}
