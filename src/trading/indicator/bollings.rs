use serde::{Deserialize, Serialize};
use ta::indicators::BollingerBands;

use crate::trading::indicator::sma::Sma;
use crate::trading::model::market::candles::CandlesEntity;

#[derive(Debug,Clone,Deserialize,Serialize)]
pub struct BollingerBandsSignalConfig {
    pub period: usize,
    pub multiplier: f64,
    pub is_open: bool,
}

impl Default for BollingerBandsSignalConfig {
    fn default() -> Self {
        Self { period: 8, multiplier: 2.0, is_open: true }
    }
}

