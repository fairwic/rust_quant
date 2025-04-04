use ta::indicators::ExponentialMovingAverage;

#[derive(Debug)]
pub struct EmaIndicator {
    pub ema1_indicator: ExponentialMovingAverage,
    pub ema2_indicator: ExponentialMovingAverage,
    pub ema3_indicator: ExponentialMovingAverage,
    pub ema4_indicator: ExponentialMovingAverage,
    pub ema5_indicator: ExponentialMovingAverage,
}
impl EmaIndicator {
    pub fn new(ema1: usize, ema2: usize, ema3: usize, ema4: usize, ema5: usize) -> Self {
        Self {
            ema1_indicator: ExponentialMovingAverage::new(ema1).unwrap(),
            ema2_indicator: ExponentialMovingAverage::new(ema2).unwrap(),
            ema3_indicator: ExponentialMovingAverage::new(ema3).unwrap(),
            ema4_indicator: ExponentialMovingAverage::new(ema4).unwrap(),
            ema5_indicator: ExponentialMovingAverage::new(ema5).unwrap(),
        }
    }
}