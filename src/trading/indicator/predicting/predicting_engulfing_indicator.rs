///预测当下一根k线 收盘价格在多少的时候，恰好形成吞没形态
use crate::CandleItem;

/// 预测吞没形态形成所需的价格点
#[derive(Debug, Clone, Default)]
pub struct EngulfingPrediction {
    /// 形成看涨吞没所需的最低收盘价
    /// (下一根 K 线的收盘价需要严格大于此值，并且满足开盘价条件)
    pub bullish_engulfing_min_close: Option<f64>,
    pub bullish_engulfing_min_open: Option<f64>,
    /// 形成看跌吞没所需的最高收盘价
    /// (下一根 K 线的收盘价需要严格小于此值，并且满足开盘价条件)
    pub bearish_engulfing_max_close: Option<f64>,
    pub bearish_engulfing_max_open: Option<f64>,
}

/// 预测吞没形态形成的指标
#[derive(Debug, Default)]
pub struct PredictingEngulfingIndicator {
    last_kline: Option<CandleItem>,
}

impl PredictingEngulfingIndicator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加最新的完整 K 线数据
    pub fn add_candle(&mut self, candle: &CandleItem) {
        self.last_kline = Some(candle.clone());
    }

    /// 获取预测结果
    /// 返回下一根 K 线需要达到的收盘价阈值才能形成吞没形态
    pub fn get_prediction(&self) -> EngulfingPrediction {
        let mut prediction = EngulfingPrediction::default();
        if let Some(last_kline) = &self.last_kline {
            // 根据 engulfing_indicator.rs 中的定义:
            // 看涨吞没条件: next_c > last_c AND next_c > last_h
            // 因此，下一个收盘价需要超过 last_c 和 last_h 中的最大值

            if last_kline.c < last_kline.o {
                //要求上一个k线是阴线
                // 看涨吞没条件: next_c > last_c AND next_c > last_h
                prediction.bullish_engulfing_min_close = Some(last_kline.c.max(last_kline.h));
                prediction.bullish_engulfing_min_open = Some(last_kline.o.max(last_kline.l));
            } else {
                //要求上一个k线是阳线
                // 看跌吞没条件: next_c < last_c AND next_c < last_l
                prediction.bearish_engulfing_max_close = Some(last_kline.c.min(last_kline.l));
                prediction.bearish_engulfing_max_open = Some(last_kline.o.min(last_kline.h));
            }
        }
        prediction
    }
    /// 重置指标状态
    pub fn reset(&mut self) {
        self.last_kline = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CandleItem;

    #[test]
    fn test_predicting_engulfing() {
        let mut indicator = PredictingEngulfingIndicator::new();

        // 初始状态，无预测
        let prediction1 = indicator.get_prediction();
        assert!(prediction1.bullish_engulfing_min_close.is_none());
        assert!(prediction1.bearish_engulfing_max_close.is_none());

        // 添加第一根 K 线
        let kline1 = CandleItem {
            o: 100.0,
            h: 110.0, // max(c, h) = 110.0
            l: 90.0,
            c: 95.0, // min(c, l) = 90.0
            ts: 0,
            v: 0.00,
        };
        indicator.add_candle(&kline1);

        let prediction2 = indicator.get_prediction();
        println!("Prediction after kline1: {:?}", prediction2);
        assert!(prediction2.bullish_engulfing_min_close == Some(110.0));
        assert!(prediction2.bullish_engulfing_min_open == Some(100.0));

        // 添加第二根 K 线 (一个阳线)
        let kline2 = CandleItem {
            o: 90.0,
            h: 105.0,
            l: 85.0,  // min(c, l) = 85.0
            c: 100.0, // max(c, h) = 105.0
            ts: 1,
            v: 0.00,
        };
        indicator.add_candle(&kline2);
        let prediction3 = indicator.get_prediction();
        println!("Prediction after kline2: {:?}", prediction3);
        assert_eq!(prediction3.bearish_engulfing_max_close, Some(85.0));
        assert_eq!(prediction3.bearish_engulfing_max_open, Some(90.0));

        // 测试重置
        indicator.reset();
        let prediction4 = indicator.get_prediction();
        assert!(prediction4.bullish_engulfing_min_close.is_none());
        assert!(prediction4.bearish_engulfing_max_close.is_none());
    }
}
