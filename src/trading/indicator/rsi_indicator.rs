// use crate::trading::indicator::rsi_rma::Rma;
use crate::trading::indicator::rma::Rma;
/// RSI indicator that uses RMA for calculations (TradingView-style)
/// 
/// This is a wrapper around RsiRma that provides a cleaner interface
/// for using RSI in trading strategies.
#[derive(Debug)]
pub struct RsiIndicator {
    rsi: Rma,
    overbought_level: f64,
    oversold_level: f64,
}

impl RsiIndicator {
    pub fn new(length: usize) -> Self {
        Self {
            rsi: Rma::new(length),
            overbought_level: 70.0,
            oversold_level: 30.0,
        }
    }

    /// Create a new RSI indicator with custom overbought and oversold levels
    pub fn new_with_levels(length: usize, overbought: f64, oversold: f64) -> Self {
        Self {
            rsi: Rma::new(length),
            overbought_level: overbought,
            oversold_level: oversold,
        }
    }

    /// Calculate the next RSI value
    pub fn next(&mut self, price: f64) -> f64 {
        self.rsi.next(price)
    }

    /// Check if RSI is in overbought territory
    pub fn is_overbought(&self, value: f64) -> bool {
        value >= self.overbought_level
    }

    /// Check if RSI is in oversold territory
    pub fn is_oversold(&self, value: f64) -> bool {
        value <= self.oversold_level
    }

    /// Set custom overbought level
    pub fn set_overbought_level(&mut self, level: f64) {
        self.overbought_level = level;
    }

    /// Set custom oversold level
    pub fn set_oversold_level(&mut self, level: f64) {
        self.oversold_level = level;
    }
} 