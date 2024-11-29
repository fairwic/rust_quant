use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqueezeConfig {
    pub bb_length: usize,
    pub bb_multi: f64,
    pub kc_length: usize,
    pub kc_multi: f64,
}

impl Default for SqueezeConfig {
    fn default() -> Self {
        Self {
            bb_length: 20,
            bb_multi: 2.0,
            kc_length: 20,
            kc_multi: 1.5,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SqueezeState {
    SqueezeOn,
    SqueezeOff,
    NoSqueeze,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum MomentumColor {
    Lime,   // 上涨加速
    Green,  // 上涨减速
    Red,    // 下跌加速
    Maroon, // 下跌减速
}

#[derive(Debug)]
pub struct SqueezeResult {
    pub timestamp: i64,
    pub close: f64,
    pub upper_bb: f64,
    pub lower_bb: f64,
    pub upper_kc: f64,
    pub lower_kc: f64,
    pub momentum: f64,
    pub momentum_color: MomentumColor,
    pub squeeze_state: SqueezeState,
}