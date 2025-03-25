use serde::{Deserialize, Serialize};
use ta::indicators::BollingerBands;

use crate::trading::indicator::sma::Sma;
use crate::trading::model::market::candles::CandlesEntity;

#[derive(Debug,Clone)]
pub struct BollingerBandsSignalConfig {
    pub period: usize,
    pub multiplier: f64,
    pub is_open: bool,
}


impl  Serialize for BollingerBandsSignalConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        serializer.serialize_str(&format!("BB: {:?}, {:?}", self.period, self.multiplier))
    }
}

impl<'de> Deserialize<'de> for BollingerBandsSignalConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut bb = Self::default();
        Ok(bb)
    }
}

impl Default for BollingerBandsSignalConfig {
    fn default() -> Self {
        Self { period: 12, multiplier: 3.0, is_open: true }
    }
}

