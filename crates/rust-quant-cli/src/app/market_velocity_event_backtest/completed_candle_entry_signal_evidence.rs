use super::MarketVelocityTradeDirection;

/// 严格由信号时点已完成 K 线计算的入场证据，落库后可复核量比与三个指标分支。
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedCandleEntrySignalEvidence {
    /// 当前成交量除以剔除历史放量后的十根基线均量。
    pub filtered_volume_ratio: f64,
    /// 十根历史样本剔除已标记放量后实际保留的根数。
    pub filtered_volume_retained_candles: usize,
    /// 当前分表 K 线的 `vol_ccy`；仅要求周基础成交量门槛的版本有值。
    pub current_volume_ccy: Option<f64>,
    /// 前 672 根 `vol_ccy` 的 nearest-rank P90；旧版本为空。
    pub weekly_volume_ccy_p90: Option<f64>,
    /// 信号 K 线 RSI14。
    pub rsi14: f64,
    /// 信号 K 线 MACD DIF。
    pub macd_dif: f64,
    /// 信号 K 线 EMA12。
    pub ema12: f64,
    /// 信号 K 线 EMA144。
    pub ema144: f64,
    /// 信号 K 线 EMA169；v12 用于复核中长期均线组顺序。
    pub ema169: f64,
    /// 信号 K 线 EMA696。
    pub ema696: f64,
    /// 信号 K 线收盘确认的 ATR14，用于成交后重建风险与固定 ATR 止盈。
    pub atr14: f64,
    /// 固定止盈距离的 ATR 倍数；V1/V2 仍按目标 R，因此为空。
    pub take_profit_atr_multiplier: Option<f64>,
    /// RSI 吞没或长影线是否实际贡献最终同方向候选，从而启用形态止损。
    pub rsi_pattern_stop_participated: bool,
    /// RSI 背离比较对；`comparison_mode` 区分 v3 枢轴确认与 v9 放量锚点。
    pub rsi_divergences: Vec<RsiDivergenceSignalEvidence>,
    /// v2 MACD 候选使用的已确认枢轴对；v1 或非 MACD 候选时为空。
    pub macd_divergences: Vec<MacdDivergenceSignalEvidence>,
    /// v4 的 BB(12, 2.6) 冲突缓冲快照；旧版本为空，且该指标不能独立创建交易。
    pub bollinger_conflict: Option<BollingerConflictSignalEvidence>,
    /// v5 信号收盘价与 EMA144 的绝对距离除以 ATR14；旧版本为空。
    pub ema144_distance_atr: Option<f64>,
    /// v5 允许 EMA 延续候选距离 EMA144 的最大 ATR 倍数；旧版本为空。
    pub ema144_max_distance_atr: Option<f64>,
    /// EMA 延续形态是否存在但因距离过远被 v5 门禁剔除；RSI/MACD 分支不受影响。
    pub ema_candidate_blocked_by_distance: bool,
    /// v11 锚点完成后采用的成交方式与实际成交证据；旧版本为空。
    pub anchor_entry: Option<AnchorEntrySignalEvidence>,
    /// v12 在锚点 p 完成时冻结的趋势、交易关系和止盈选择；旧版本为空。
    pub trend_managed_exit: Option<TrendManagedExitSignalEvidence>,
    /// 本轮拆分后的单一入场假设证据；旧混合版本为空。
    pub isolated_family: Option<IsolatedStrategyFamilySignalEvidence>,
}

/// 三个独立研究家族在信号时点冻结的最小假设证据。
#[derive(Debug, Clone, PartialEq)]
pub struct IsolatedStrategyFamilySignalEvidence {
    /// `momentum_exhaustion_reversal`、`volume_anchor_rsi_divergence` 或 `volume_platform_break_trend`。
    pub family: &'static str,
    /// 人工冻结且可检索的最小假设标识。
    pub hypothesis: &'static str,
    /// 动量衰竭家族使用的 p 前 96 根有符号净移动；其他家族为空。
    pub prior_96_net_move_pct: Option<f64>,
    /// 趋势家族在两根接受确认完成时冻结的平台破位证据；其他家族为空。
    pub platform_breakdown: Option<PlatformBreakdownSignalEvidence>,
    /// 趋势家族是否已经通过同方向三根均线顺序与 EMA696 斜率确认。
    pub long_term_ema_confirmed: bool,
    /// 趋势家族用于复核 EMA696 三次同向变化的最近四个值。
    pub ema696_recent: Vec<f64>,
}

/// v12 的平台破位证据；只记录在信号时点仍未被后续收盘收回的最近一次确认。
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformBreakdownSignalEvidence {
    /// `bearish` 或 `bullish`。
    pub direction: &'static str,
    /// 破位 K 的开始时间。
    pub break_ts_ms: i64,
    /// 两根保持确认完成的时间；该时点之前不能激活平台趋势。
    pub confirmed_ts_ms: i64,
    /// 破位前 20 根 K 线最高价。
    pub platform_high: f64,
    /// 破位前 20 根 K 线最低价。
    pub platform_low: f64,
    /// 平台完整宽度除以版本冻结的 ATR14；V2 改为破位前一根 ATR。
    pub platform_range_atr: f64,
    /// 平台宽度所用 ATR 的 K 线开始时间；V1 与旧趋势管理版本为空。
    pub atr_reference_ts_ms: Option<i64>,
    /// 平台宽度所用 ATR14 原值；V1 与旧趋势管理版本为空。
    pub platform_reference_atr14: Option<f64>,
    /// 前五根与后五根收盘均值偏移除以参考 ATR；仅平台 V2 有值。
    pub close_center_shift_atr: Option<f64>,
    /// 20 根平台收盘线性回归的决定系数；仅平台 V2 有值。
    pub close_regression_r_squared: Option<f64>,
    /// 回归拟合首尾绝对漂移除以参考 ATR；仅平台 V2 有值。
    pub fitted_close_drift_atr: Option<f64>,
    /// 落入上沿 10% 宽度触碰区的 K 线数；仅平台 V2 有值。
    pub upper_touch_count: Option<usize>,
    /// 落入下沿 10% 宽度触碰区的 K 线数；仅平台 V2 有值。
    pub lower_touch_count: Option<usize>,
    /// 破位 K 实体占完整振幅的比例。
    pub break_body_range_ratio: f64,
    /// 破位 K 实体相对开盘价的比例。
    pub break_body_open_ratio: f64,
    /// 破位 K 按自身时点计算的过滤量比。
    pub filtered_volume_ratio: f64,
    /// 破位 K 的 `vol_ccy`。
    pub current_volume_ccy: f64,
    /// 破位 K 之前 672 根 `vol_ccy` 的 nearest-rank P90。
    pub weekly_volume_ccy_p90: f64,
}

/// v12+ 在锚点 p 完成时冻结的趋势分类与止盈政策。
#[derive(Debug, Clone, PartialEq)]
pub struct TrendManagedExitSignalEvidence {
    /// `bearish`、`bullish`、`neutral` 或 `conflict_neutral`。
    pub market_regime: &'static str,
    /// 当前交易相对趋势为 `with_trend`、`countertrend` 或 `neutral`。
    pub trade_trend_relation: &'static str,
    /// 最近三根均线有序且 EMA696 连续三次下降。
    pub long_term_bearish_confirmed: bool,
    /// 最近三根均线有序且 EMA696 连续三次上升。
    pub long_term_bullish_confirmed: bool,
    /// 最近四个 EMA696 值，用于复核三次连续斜率。
    pub ema696_recent: Vec<f64>,
    /// 信号时点仍有效的最近看空平台破位。
    pub bearish_platform_breakdown: Option<PlatformBreakdownSignalEvidence>,
    /// 信号时点仍有效的最近看多平台突破。
    pub bullish_platform_breakdown: Option<PlatformBreakdownSignalEvidence>,
    /// p 前 96 根从最早开盘到最后收盘的有符号净变化百分比。
    pub prior_96_net_move_pct: Option<f64>,
    /// 逆势交易是否满足量比至少 4 且 96 根沿趋势方向至少 8% 的例外。
    pub countertrend_extreme_move_exception: bool,
    /// 未应用逆势缩短前，V11 按信号量比选出的 ATR 目标。
    pub volume_tier_take_profit_atr_multiplier: f64,
    /// 当前规则版本最终冻结的 ATR 止盈倍数。
    pub selected_take_profit_atr_multiplier: f64,
    /// `volume_tier`、版本化逆势默认目标或 `countertrend_extreme_volume_tier`。
    pub target_policy: &'static str,
}

/// v11 在锚点完成时冻结触发线，并在紧邻下一根 15m K 线补全成交证据。
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorEntrySignalEvidence {
    /// `pivot_directional_wick_next_open` 或 `next_candle_intrabar_break`。
    pub activation_mode: &'static str,
    /// 锚点实体占完整振幅的比例，用于复核十字星排除规则。
    pub pivot_body_range_ratio: f64,
    /// 候选方向影线占完整振幅的比例。
    pub pivot_directional_wick_range_ratio: f64,
    /// 候选反方向影线占完整振幅的比例。
    pub pivot_opposite_wick_range_ratio: f64,
    /// 非影线分支的突破线；做多为 p.high，做空为 p.low。
    pub activation_price: f64,
    /// 唯一允许触发的紧邻下一根 15m K 线开始时间；成交前为空。
    pub activation_candle_ts_ms: Option<i64>,
    /// 按下一根开盘或盘中突破线确定的原始成交价；成交前为空。
    pub fill_price: Option<f64>,
    /// 区分影线下一开盘、跳空越线开盘和盘中触价。
    pub fill_price_source: Option<&'static str>,
    /// 仅 OHLC 可见时，同根触发/止损/止盈冲突采用的保守回放政策。
    pub intrabar_path_policy: Option<&'static str>,
}

/// v4 在信号收盘时冻结的布林带与触轨结果，用于证明冲突缓冲没有读取未来 K 线。
#[derive(Debug, Clone, PartialEq)]
pub struct BollingerConflictSignalEvidence {
    /// 参与计算的已完成 15m 收盘价根数。
    pub period: usize,
    /// 总体标准差的倍数。
    pub standard_deviation_multiplier: f64,
    /// 当前信号 K 线完成后的中轨。
    pub middle: f64,
    /// 当前信号 K 线完成后的上轨。
    pub upper: f64,
    /// 当前信号 K 线完成后的下轨。
    pub lower: f64,
    /// 当前 K 线最高价是否触达或越过上轨；仅用于抵消已有做多候选。
    pub touches_upper: bool,
    /// 当前 K 线最低价是否触达或越过下轨；仅用于抵消已有做空候选。
    pub touches_lower: bool,
}

/// RSI 背离在确认时点冻结的比较对，避免只凭 trigger 无法复核不同版本语义。
#[derive(Debug, Clone, PartialEq)]
pub struct RsiDivergenceSignalEvidence {
    /// 稳定模式名，用于区分价格枢轴、即时放量锚点和下一根收盘确认。
    pub comparison_mode: &'static str,
    /// 顶背离为空、底背离为多。
    pub direction: MarketVelocityTradeDirection,
    /// v3 为右侧三根确认的枢轴 p；v9 为当前已完成比较 K。
    pub pivot_ts_ms: i64,
    /// p 前 48 根内最近同类枢轴或方向性放量锚点 q。
    pub reference_pivot_ts_ms: i64,
    /// 按方向选择的 p 最高价或最低价。
    pub pivot_price: f64,
    /// 按方向选择的 q 最高价或最低价。
    pub reference_pivot_price: f64,
    /// 枢轴 p 的 RSI14。
    pub pivot_rsi14: f64,
    /// 枢轴 q 的 RSI14。
    pub reference_pivot_rsi14: f64,
    /// v9 当前比较 K 的过滤量比；v3 为空。
    pub pivot_filtered_volume_ratio: Option<f64>,
    /// v9 历史锚点 K 按自己时点计算的过滤量比；v3 为空。
    pub reference_filtered_volume_ratio: Option<f64>,
    /// v9 当前比较 K 的 `vol_ccy`；v3 为空。
    pub pivot_volume_ccy: Option<f64>,
    /// v9 历史锚点 K 的 `vol_ccy`；v3 为空。
    pub reference_volume_ccy: Option<f64>,
    /// v9 当前比较 K 的前 672 根 `vol_ccy` P90；v3 为空。
    pub pivot_weekly_volume_ccy_p90: Option<f64>,
    /// v9 历史锚点 K 自己时点的前 672 根 `vol_ccy` P90；v3 为空。
    pub reference_weekly_volume_ccy_p90: Option<f64>,
    /// v10 紧邻 p 的确认 K 时间；v3/v9 为空，避免改变历史明细 JSON。
    pub confirmation_ts_ms: Option<i64>,
    /// v10 确认 K 的收盘价；做多必须高于 p.high，做空必须低于 p.low。
    pub confirmation_close: Option<f64>,
    /// v10 按方向冻结的突破线，即做多 p.high 或做空 p.low。
    pub confirmation_break_price: Option<f64>,
}

/// MACD v2 在信号时点冻结的枢轴对和阈值证据，用于从明细还原每个条件。
#[derive(Debug, Clone, PartialEq)]
pub struct MacdDivergenceSignalEvidence {
    /// 候选方向；空表示顶背离，多表示底背离。
    pub direction: MarketVelocityTradeDirection,
    /// 刚在当前收盘确认的枢轴 p，Unix 毫秒时间戳。
    pub pivot_ts_ms: i64,
    /// p 之前 48 根内最近同类枢轴 q，Unix 毫秒时间戳。
    pub reference_pivot_ts_ms: i64,
    /// 按方向选取的 p 最高价或最低价。
    pub pivot_price: f64,
    /// 按方向选取的 q 最高价或最低价。
    pub reference_pivot_price: f64,
    /// 枢轴 p 上的 RSI14；顶背离要求不低于 70，底背离要求不高于 30。
    pub pivot_rsi14: f64,
    /// 枢轴 p 上的 DIF。
    pub pivot_dif: f64,
    /// 枢轴 q 上的 DIF。
    pub reference_pivot_dif: f64,
    /// 枢轴 p 上的 DIF/收盘价。
    pub pivot_normalized_dif: f64,
    /// 枢轴 q 上的 DIF/收盘价。
    pub reference_pivot_normalized_dif: f64,
    /// 两个枢轴沿候选方向实际达到的归一化 DIF 改善幅度。
    pub normalized_dif_improvement: f64,
    /// 零轴缓冲系数 Z，零轴带为各枢轴的 Z×ATR14。
    pub zero_band_atr_multiplier: f64,
    /// 本次实验要求的最小归一化 DIF 改善幅度 D_min。
    pub min_normalized_dif_improvement: f64,
}
