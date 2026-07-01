use rust_quant_domain::{SignalDirection, SignalResult};
use serde::{Deserialize, Serialize};

/// 网格交易 Scalper：在窄幅震荡区间内高频买卖赚价差。
/// 核心逻辑：
/// - 检测价格进入网格区间（上限下限由ATR或百分比定义）
/// - 在网格内每隔固定间距放置买卖单
/// - 每次成交后立即在对手方向开仓，赚取固定价差
/// - 趋势突破网格时熔断（stop all / 反向开仓跟随趋势）
///
/// 目标：超高频（日均30-50单）+ 超高胜率（85-95%）+ 累积收益
///
/// 风险：单边突破时可能连续止损，需要趋势过滤和熔断机制。

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GridAction {
    BuyGrid,        // 在网格下方买入
    SellGrid,       // 在网格上方卖出
    EmergencyClose, // 趋势突破，紧急平仓
    Flat,           // 无操作
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridScalperThresholds {
    /// 网格区间宽度（百分比），例如 0.02 = 2%
    pub grid_width_pct: f64,
    /// 网格内分档数量（越多越密集，频次越高）
    pub grid_levels: usize,
    /// 单档利润目标（百分比），例如 0.003 = 0.3%
    pub profit_per_level_pct: f64,
    /// 单档止损（百分比），例如 0.005 = 0.5%（略大于利润）
    pub stop_per_level_pct: f64,
    /// 趋势熔断阈值：价格偏离网格中心超过此倍数ATR时停止网格
    pub trend_break_atr_mult: f64,
    /// 最小震荡检测周期：需要N根K线内波动<X%才认为在震荡
    pub ranging_lookback: usize,
    /// 震荡判定阈值：N根K线内最高最低价差<此百分比
    pub ranging_threshold_pct: f64,
    /// 网格冷却期（根K线数）：平仓后等待N根再重新开网格
    pub grid_cooldown: usize,
}

impl Default for GridScalperThresholds {
    fn default() -> Self {
        Self {
            grid_width_pct: 0.015,       // 1.5% 区间
            grid_levels: 5,              // 5档，每档0.3%
            profit_per_level_pct: 0.003, // 0.3% 利润
            stop_per_level_pct: 0.006,   // 0.6% 止损（1:2 盈亏比，靠胜率补）
            trend_break_atr_mult: 2.5,   // 2.5倍ATR偏离触发熔断
            ranging_lookback: 20,        // 检测最近20根
            ranging_threshold_pct: 0.02, // 波动<2%认为震荡
            grid_cooldown: 3,            // 平仓后冷却3根K线
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridScalperSignalSnapshot {
    pub price: f64,
    pub atr: f64,
    pub grid_center: f64,         // 网格中心（震荡区间的中点）
    pub grid_upper: f64,          // 网格上限
    pub grid_lower: f64,          // 网格下限
    pub in_ranging_mode: bool,    // 当前是否处于震荡模式
    pub price_to_center_pct: f64, // 价格偏离中心的百分比
    pub recent_range_pct: f64,    // 最近N根K线的波动百分比
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridScalperDecision {
    pub action: GridAction,
    pub reasons: Vec<String>,
}

impl GridScalperDecision {
    pub fn to_signal(&self, price: f64, thresholds: &GridScalperThresholds) -> SignalResult {
        let mut signal = SignalResult::default();
        signal.ts = chrono::Utc::now().timestamp_millis();
        signal.open_price = price;

        match self.action {
            GridAction::BuyGrid => {
                signal.should_buy = true;
                signal.direction = SignalDirection::Long;
                // 止损：价格下跌 stop_per_level_pct
                let stop = price * (1.0 - thresholds.stop_per_level_pct);
                signal.signal_kline_stop_loss_price = Some(stop);
                signal.stop_loss_source = Some("GridScalper".to_string());
                // 止盈：价格上涨 profit_per_level_pct
                let target = price * (1.0 + thresholds.profit_per_level_pct);
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            GridAction::SellGrid => {
                signal.should_sell = true;
                signal.direction = SignalDirection::Short;
                let stop = price * (1.0 + thresholds.stop_per_level_pct);
                signal.signal_kline_stop_loss_price = Some(stop);
                signal.stop_loss_source = Some("GridScalper".to_string());
                let target = price * (1.0 - thresholds.profit_per_level_pct);
                signal.atr_take_profit_level_1 = Some(target);
                signal.atr_take_profit_level_2 = Some(target);
                signal.atr_take_profit_level_3 = Some(target);
            }
            GridAction::EmergencyClose => {
                // 紧急平仓信号（策略外处理）
                signal.should_buy = false;
                signal.should_sell = false;
            }
            GridAction::Flat => {}
        }

        signal
    }
}

/// 回测调参配置（与 live 的 thresholds 分离）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GridScalperBacktestTuning {
    pub atr_period: usize,
    pub grid_width_pct: f64,
    pub grid_levels: usize,
    pub profit_per_level_pct: f64,
    pub stop_per_level_pct: f64,
    pub trend_break_atr_mult: f64,
    pub ranging_lookback: usize,
    pub ranging_threshold_pct: f64,
    pub grid_cooldown: usize,
}

impl Default for GridScalperBacktestTuning {
    fn default() -> Self {
        let t = GridScalperThresholds::default();
        Self {
            atr_period: 14,
            grid_width_pct: t.grid_width_pct,
            grid_levels: t.grid_levels,
            profit_per_level_pct: t.profit_per_level_pct,
            stop_per_level_pct: t.stop_per_level_pct,
            trend_break_atr_mult: t.trend_break_atr_mult,
            ranging_lookback: t.ranging_lookback,
            ranging_threshold_pct: t.ranging_threshold_pct,
            grid_cooldown: t.grid_cooldown,
        }
    }
}

impl GridScalperBacktestTuning {
    pub fn thresholds(&self) -> GridScalperThresholds {
        GridScalperThresholds {
            grid_width_pct: self.grid_width_pct,
            grid_levels: self.grid_levels,
            profit_per_level_pct: self.profit_per_level_pct,
            stop_per_level_pct: self.stop_per_level_pct,
            trend_break_atr_mult: self.trend_break_atr_mult,
            ranging_lookback: self.ranging_lookback,
            ranging_threshold_pct: self.ranging_threshold_pct,
            grid_cooldown: self.grid_cooldown,
        }
    }
}
