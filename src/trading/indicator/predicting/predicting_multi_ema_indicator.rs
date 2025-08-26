use std::collections::HashMap;
use ta::errors::TaError;
use ta::indicators::ExponentialMovingAverage;
use ta::{Next, Reset};

/// EMA 预测结果
#[derive(Debug, Clone, Default)]
pub struct EmaPrediction {
    /// 存储每个 EMA 周期及其对应的预测触碰价格 (即当前 EMA 值)
    pub touch_prices: HashMap<usize, f64>,
}

/// 预测多条 EMA 触碰点的指标
#[derive(Debug, Clone)]
pub struct PredictingMultiEmaIndicator {
    emas: HashMap<usize, ExponentialMovingAverage>,
    last_predictions: EmaPrediction,
}

impl PredictingMultiEmaIndicator {
    /// 创建一个新的实例，包含指定周期的 EMA
    ///
    /// # Arguments
    ///
    /// * `periods` - 一个包含所需 EMA 周期的 slice
    ///
    /// # Errors
    ///
    /// 如果 `periods` 包含 0，或者无法创建 EMA 指标，则返回 `TaError`。
    pub fn new(periods: &[usize]) -> Result<Self, TaError> {
        let mut emas = HashMap::with_capacity(periods.len());
        for &period in periods {
            // ta 库不允许周期为 0，并且 ExponentialMovingAverage::new 会处理此情况并返回错误
            // 我们直接尝试创建，让 ta 库处理错误
            let ema = ExponentialMovingAverage::new(period)?;
            emas.insert(period, ema);
        }
        Ok(Self {
            emas,
            last_predictions: EmaPrediction::default(),
        })
    }

    /// 使用新的价格更新所有 EMA 指标
    pub fn next(&mut self, price: f64) {
        let mut touch_prices = HashMap::with_capacity(self.emas.len());
        for (&period, ema) in self.emas.iter_mut() {
            let ema_value = ema.next(price);
            touch_prices.insert(period, ema_value);
        }
        self.last_predictions = EmaPrediction { touch_prices };
    }

    /// 获取最新的 EMA 预测触碰价格
    /// 返回一个 HashMap，其中 key 是 EMA 周期，value 是该 EMA 的当前值
    #[inline]
    pub fn get_predictions(&self) -> &EmaPrediction {
        &self.last_predictions
    }

    /// 获取特定周期的 EMA 预测触碰价格
    #[inline]
    pub fn get_prediction_for_period(&self, period: usize) -> Option<f64> {
        self.last_predictions.touch_prices.get(&period).copied()
    }

    /// 重置所有 EMA 指标
    pub fn reset(&mut self) {
        for ema in self.emas.values_mut() {
            ema.reset();
        }
        self.last_predictions = EmaPrediction::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 定义一个小的容差用于浮点数比较
    const F64_EPSILON: f64 = 1e-4;

    fn setup_indicator(periods: &[usize]) -> PredictingMultiEmaIndicator {
        PredictingMultiEmaIndicator::new(periods).expect("Indicator creation failed")
    }

    fn add_prices(indicator: &mut PredictingMultiEmaIndicator, prices: &[f64]) {
        for &price in prices {
            indicator.next(price);
        }
    }

    #[test]
    fn test_predicting_multi_ema_values() {
        let periods = vec![5, 10];
        let mut indicator = setup_indicator(&periods);

        let initial_prices = vec![
            10.0, 11.0, 12.0, 13.0, 14.0, // EMA5 ready
            15.0, 16.0, 17.0, 18.0, 19.0, // EMA10 ready
            20.0,
        ];
        add_prices(&mut indicator, &initial_prices);

        // 第一次检查
        let predictions1 = indicator.get_predictions();
        println!("EMA Predictions after initial prices: {:?}", predictions1);
        assert!(predictions1.touch_prices.contains_key(&5));
        assert!(predictions1.touch_prices.contains_key(&10));
        let ema5_1 = indicator.get_prediction_for_period(5).unwrap();
        let ema10_1 = indicator.get_prediction_for_period(10).unwrap();
        println!("EMA 5 Touch Price 1: {}", ema5_1);
        println!("EMA 10 Touch Price 1: {}", ema10_1);
        // 这些值是通过手动运行 ta 库确认的，或者根据你的精确计算
        // EMA(5) @ 20.0 -> (14+15+16+17+18+19+20) => ~17.18 (SMA-like first, then EMA)
        // ... 具体值需要精确计算或依赖 ta 库行为

        // 添加一个价格并检查 EMA5
        indicator.next(18.0346);
        let ema5_2 = indicator.get_prediction_for_period(5).unwrap();
        println!("EMA 5 Touch Price 2: {}", ema5_2);
        // 使用近似比较代替字符串比较
        assert!(
            (ema5_2 - 18.0340).abs() < F64_EPSILON,
            "EMA5 value mismatch"
        ); // 假设精确值为 18.0340

        // 添加另一个价格并检查 EMA10
        // 需要重新初始化以获得精确的 EMA10 值
        let mut indicator2 = setup_indicator(&periods);
        add_prices(&mut indicator2, &initial_prices);
        indicator2.next(16.1049);
        let ema10_3 = indicator2.get_prediction_for_period(10).unwrap();
        println!("EMA 10 Touch Price 3: {}", ema10_3);
        // 使用近似比较代替字符串比较
        assert!(
            (ema10_3 - 16.1044).abs() < F64_EPSILON,
            "EMA10 value mismatch"
        ); // 假设精确值为 16.1044
    }

    #[test]
    fn test_reset_functionality() {
        let periods = vec![5, 10];
        let mut indicator = setup_indicator(&periods);

        add_prices(&mut indicator, &[10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);

        let predictions_before_reset = indicator.get_predictions();
        assert!(predictions_before_reset.touch_prices.contains_key(&5));

        indicator.reset();
        let predictions_after_reset = indicator.get_predictions();
        assert!(predictions_after_reset.touch_prices.is_empty());

        // 验证重置后重新计算
        add_prices(&mut indicator, &[10.0, 11.0, 12.0, 13.0, 14.0]); // 重新添加足够的数据
        let predictions_after_recalc = indicator.get_predictions();
        assert!(predictions_after_recalc.touch_prices.contains_key(&5));
        let ema5_recalc = indicator.get_prediction_for_period(5).unwrap();
        // EMA5 of (10,11,12,13,14) is the SMA = (10+11+12+13+14)/5 = 12.0
        assert!(
            (ema5_recalc - 12.0).abs() < F64_EPSILON,
            "EMA5 after reset mismatch"
        );
    }

    #[test]
    fn test_new_with_zero_period() {
        let periods = vec![5, 0, 10];
        let result = PredictingMultiEmaIndicator::new(&periods);
        assert!(result.is_err());
        match result {
            Err(TaError::InvalidParameter) => {
                // Correctly match the unit-like variant
            }
            Err(e) => panic!("Expected InvalidParameter error, but got {:?}", e),
            Ok(_) => panic!("Expected an error, but got Ok"),
        }
    }

    #[test]
    fn test_new_with_empty_period() {
        let periods: Vec<usize> = vec![];
        let mut indicator = setup_indicator(&periods); // Make indicator mutable
        assert!(indicator.emas.is_empty());
        assert!(indicator.last_predictions.touch_prices.is_empty());
        indicator.next(10.0); // Now this is allowed
        assert!(indicator.last_predictions.touch_prices.is_empty());
    }
}
