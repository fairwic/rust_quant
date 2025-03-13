use serde::{Deserialize, Serialize};
use ta::indicators::BollingerBands;

use crate::trading::indicator::sma::Sma;
use crate::trading::model::market::candles::CandlesEntity;

#[derive(Debug,Clone,Default)]
pub struct BollingerBandsSignal {
    pub bolling_bands: BollingerBands,
    pub multiplier : f64,
    pub is_open: bool,
}

impl  Serialize for BollingerBandsSignal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        serializer.serialize_str(&format!("BB: {:?}", self.bolling_bands))
    }
}

impl<'de> Deserialize<'de> for BollingerBandsSignal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut bb = Self::default();
        bb.bolling_bands = BollingerBands::new(20, 2.0).unwrap();
        bb.is_open = true;
        Ok(bb)
    }
}
impl BollingerBandsSignal {
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self { bolling_bands: BollingerBands::new(20, multiplier).unwrap(), multiplier, is_open: true }
    }
}
