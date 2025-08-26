use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqueezeConfig {
    pub bb_length: usize,
    pub bb_multi: f64,
    pub kc_length: usize,
    pub kc_multi: f64,
}
impl Display for SqueezeConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bb_length:{} bb_multi:{} kc_length:{} kc_multi:{}",
            self.bb_length, self.bb_multi, self.kc_length, self.kc_multi
        )
    }
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
    pub momentum: Vec<f64>,
    pub momentum_color: MomentumColor,
    pub squeeze_state: SqueezeState,
}
