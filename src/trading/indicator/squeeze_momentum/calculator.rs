use anyhow::Result;
use serde::{Deserialize, Serialize};
use ta::{
    indicators::{BollingerBands, SimpleMovingAverage, TrueRange},
    Close, DataItem, High, Low, Next,
};
use tracing::info;

use crate::trading::indicator::squeeze_momentum::service::calculate_linreg;
use crate::trading::indicator::squeeze_momentum::types::{
    MomentumColor, SqueezeConfig, SqueezeResult, SqueezeState,
};

pub struct SqueezeCalculator {
    config: SqueezeConfig,
    bb: BollingerBands,
    ma: SimpleMovingAverage,
    tr: TrueRange,
    range_ma: SimpleMovingAverage,
}

impl SqueezeCalculator {
    pub fn new(config: SqueezeConfig) -> Result<Self> {
        println!("config new:{:?}", config);
        let squeeze = Self {
            //这里bb传入的是kc的系数
            bb: BollingerBands::new(config.bb_length, config.kc_multi)?,
            ma: SimpleMovingAverage::new(config.kc_length)?,
            tr: TrueRange::new(),
            range_ma: SimpleMovingAverage::new(config.kc_length)?,
            config,
        };
        Ok(squeeze)
    }

    pub fn determine_momentum_color(&self, val: f64, prev_val: Option<f64>) -> MomentumColor {
        if val > 0.0 {
            if let Some(prev) = prev_val {
                if val > prev {
                    MomentumColor::Lime
                } else {
                    MomentumColor::Green
                }
            } else {
                MomentumColor::Green
            }
        } else {
            if let Some(prev) = prev_val {
                if val < prev {
                    MomentumColor::Red
                } else {
                    MomentumColor::Maroon
                }
            } else {
                MomentumColor::Maroon
            }
        }
    }
    pub fn calculate_momentum(&self, closes: &[f64], highs: &[f64], lows: &[f64]) -> Result<f64> {
        let period_highest = highs
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap();
        let period_lowest = lows
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap();
        let period_avg_hl = (period_highest + period_lowest) / 2.0;

        let mut kc_ma = SimpleMovingAverage::new(self.config.kc_length)?;
        let period_kc_ma = closes.iter().fold(0.0, |_, &close| {
            kc_ma.next(
                &DataItem::builder()
                    .close(close)
                    .open(close)
                    .high(close)
                    .low(close)
                    .volume(0.0)
                    .build()
                    .unwrap(),
            )
        });

        let period_avg_final = (period_avg_hl + period_kc_ma) / 2.0;
        let momentum_source: Vec<f64> = closes
            .iter()
            .map(|&close| close - period_avg_final)
            .collect();

        Ok(calculate_linreg(&momentum_source, self.config.kc_length, 0).unwrap())
    }

    pub fn calculate(&mut self, data: &[DataItem]) -> Result<SqueezeResult> {
        if data.len() < self.config.kc_length {
            return Err(anyhow::anyhow!("Insufficient data points"));
        }

        let mut closes = Vec::with_capacity(self.config.kc_length);
        let mut highs = Vec::with_capacity(self.config.kc_length);
        let mut lows = Vec::with_capacity(self.config.kc_length);

        let mut last_bb = None;

        let mut ma = 0.0;
        let mut range_ma = 0.0;

        //计算布林带
        let window = &data[data.len() - self.config.bb_length..];
        info!("bb windows length {:?}", window.len());
        for item in window {
            last_bb = Some(self.bb.next(item));
        }

        //计算kc
        let window = &data[data.len() - self.config.kc_length..];
        info!("kc windows length {:?}", window.len());
        for item in window {
            closes.push(item.close());
            highs.push(item.high());
            lows.push(item.low());

            ma = self.ma.next(item);
            let tr_val = self.tr.next(item);
            range_ma = self.range_ma.next(
                &DataItem::builder()
                    .close(tr_val)
                    .open(tr_val)
                    .high(tr_val)
                    .low(tr_val)
                    .volume(0.0)
                    .build()?,
            );
        }

        let bb_val = last_bb.ok_or_else(|| anyhow::anyhow!("Failed to calculate BB"))?;

        let momentum = self.calculate_momentum(&closes, &highs, &lows)?;

        let upper_kc = ma + range_ma * self.config.kc_multi;
        let lower_kc = ma - range_ma * self.config.kc_multi;

        let squeeze_state = if bb_val.lower > lower_kc && bb_val.upper < upper_kc {
            SqueezeState::SqueezeOn
        } else if bb_val.lower < lower_kc && bb_val.upper > upper_kc {
            SqueezeState::SqueezeOff
        } else {
            SqueezeState::NoSqueeze
        };

        let momentum_color =
            self.determine_momentum_color(momentum, closes.get(closes.len() - 2).copied());

        Ok(SqueezeResult {
            timestamp: 0, // 需要外部设置
            close: *closes.last().unwrap(),
            upper_bb: bb_val.upper,
            lower_bb: bb_val.lower,
            upper_kc,
            lower_kc,
            momentum,
            momentum_color,
            squeeze_state,
        })
    }
}
