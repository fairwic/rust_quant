use rust_quant_market::models::CandlesEntity;
use ta::{Close, High, Low};

pub struct KDJ {
    pub(crate) k: f64,
    pub(crate) d: f64,
    pub(crate) j: f64,
}

impl KDJ {
    /// 获取K值
    pub fn k(&self) -> f64 {
        self.k
    }

    /// 获取D值
    pub fn d(&self) -> f64 {
        self.d
    }

    /// 获取J值
    pub fn j(&self) -> f64 {
        self.j
    }
}

pub struct KdjCandle {
    pub(crate) high: f64,
    pub(crate) low: f64,
    pub(crate) close: f64,
}

impl High for KdjCandle {
    fn high(&self) -> f64 {
        self.high
    }
}

impl Low for KdjCandle {
    fn low(&self) -> f64 {
        self.low
    }
}

impl Close for KdjCandle {
    fn close(&self) -> f64 {
        self.close
    }
}

pub struct KdjSimpleIndicator {}

impl KdjSimpleIndicator {
    pub fn calculate_kdj_with_bcwsma(
        candles: &[CandlesEntity],
        period: usize,
        signal_period: usize,
    ) -> Vec<KDJ> {
        let mut kdjs = Vec::with_capacity(candles.len());
        let mut k = 50.0;
        let mut d = 50.0;

        // 定义BCWSMA计算函数
        fn bcwsma(s: f64, l: usize, m: f64, prev: f64) -> f64 {
            (m * s + (l as f64 - m) * prev) / l as f64
        }

        // 遍历K线数据计算KDJ
        for i in 0..candles.len() {
            if i >= period - 1 {
                // 计算当前周期的最高价和最低价
                let slice = &candles[i + 1 - period..=i];
                let (mut high, mut low) = (f64::MIN, f64::MAX);

                for c in slice {
                    let h = c.h.parse::<f64>().unwrap_or(f64::MIN);
                    let l = c.l.parse::<f64>().unwrap_or(f64::MAX);
                    high = high.max(h);
                    low = low.min(l);
                }

                // 计算RSV值
                let close = candles[i].c.parse::<f64>().unwrap_or(0.0);
                let rsv = if high == low {
                    50.0
                } else {
                    (close - low) / (high - low) * 100.0
                };

                // 使用BCWSMA计算K、D、J值
                k = bcwsma(rsv, signal_period, 1.0, k);
                d = bcwsma(k, signal_period, 1.0, d);
                let j = 3.0 * k - 2.0 * d;
                kdjs.push(KDJ { k, d, j });
            } else {
                // 初始周期内使用默认值
                kdjs.push(KDJ {
                    k: 50.0,
                    d: 50.0,
                    j: 50.0,
                });
            }
        }

        kdjs
    }
}
