use rust_quant_indicators::signal_weight::SignalWeightsConfig;
use rust_quant_indicators::trend::vegas::{
    default_chase_confirm_config, default_extreme_k_filter, default_fib_retracement_signal_config,
    default_large_entity_stop_loss_config, default_macd_signal_config, BosFvgRetestConfig,
    CandleMomentumActivationConfig, ChaseConfirmConfig, CompressedRangeBreakoutConfig,
    CrossAssetAdaptiveThresholdConfig, DonchianBreakoutAcceptanceConfig,
    DonchianVolumeBreakoutConfig, EmaDistanceConfig, EmaSignalConfig, EmaTouchTrendSignalConfig,
    EmaTunnelRetestConfirmationConfig, EngulfingSignalConfig, EntryBlockConfig,
    ExtremeKFilterConfig, FibRetracementSignalConfig, FvgReclaimConfig, KlineHammerConfig,
    LegDetectionConfig, LiquiditySweepReversalConfig, MacdDivergenceReversalConfig,
    MacdSignalConfig, MacdTrendResetBosConfig, MarketStructureConfig, RangeFilterConfig,
    RsiSignalConfig, ShortProfitProtectionConfig, VegasStrategy, VolumeProfileFailedAuctionConfig,
    VolumeProfileValueAreaBreakoutConfig, VolumeProfileValueAreaRetestConfig, VolumeSignalConfig,
};
use rust_quant_indicators::volatility::BollingBandsSignalConfig;
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
#[derive(Default, Clone)]
pub struct ParamMergeBuilder {
    //bolling
    pub bb_period: i32,
    /// 布林带倍数。
    pub bb_multiplier: f64,
    //volume
    pub volume_bar_num: usize,
    /// 成交量increase 比例。
    pub volume_increase_ratio: f64,
    /// 成交量decrease 比例。
    pub volume_decrease_ratio: f64,
    /// breakthrough阈值，用于当前结构体的业务数据。
    pub breakthrough_threshold: f64,
    /// 完整 EMA 参数；为空时沿用旧参数生成器，只覆盖突破阈值。
    pub ema_signal: Option<EmaSignalConfig>,
    //rsi
    pub rsi_period: usize,
    /// RSI 超买阈值。
    pub rsi_overbought: f64,
    /// RSI 超卖阈值。
    pub rsi_oversold: f64,
    //hammer
    pub hammer_shadow_ratio: f64,
    //kline
    pub kline_start_time: Option<i64>,
    /// 结束时间。
    pub kline_end_time: Option<i64>,
    /// Vegas 的预热与信号滚动窗口 K 线根数；为空时保持既有的 3600 根。
    pub min_k_line_num: Option<usize>,
    //risk
    pub max_loss_percent: f64,  // 最大止损百分比
    pub take_profit_ratio: f64, // 盈利阈值，用于动态止盈
    // 固定信号线的止盈比例
    pub fix_signal_kline_take_profit_ratio: Option<f64>, // 固定信号线的止盈比例，比如当盈利超过 k线路的长度的 n 倍时，直接止盈，适用短线策略
    pub is_used_signal_k_line_stop_loss: bool, //是否使用最低价止损,当价格低于入场k线的最低价时,止损。或者空单的时候,价格高于入场k线的最高价时,止损
    /// 动态最大亏损开关：true 收紧异常波动止损，false 固定使用配置止损；None 为兼容旧配置仍按 true 处理。
    pub dynamic_max_loss: Option<bool>,
    /// dynamic入场amp阈值；为空时使用默认值或表示不限制。
    pub dynamic_entry_amp_threshold: Option<f64>,
    /// 动态入场亏损百分比；为空时不启用动态入场亏损过滤。
    pub dynamic_entry_loss_percent: Option<f64>,
    /// 动态入场是否要求方向不一致；为空时使用默认规则。
    pub dynamic_entry_require_direction_mismatch: Option<bool>,
    /// 动态区间threshold；为空时表示该值未提供。
    pub dynamic_range_threshold: Option<f64>,
    /// 动态区间亏损百分比；为空时不启用动态区间亏损过滤。
    pub dynamic_range_loss_percent: Option<f64>,
    /// 仓位乘数；小于 1 用于标准化非全仓回测，大于 1 用于杠杆压力测试。
    pub position_leverage: Option<f64>,
    // strategy extensions
    pub signal_weights: Option<SignalWeightsConfig>,
    /// 吞没形态配置；显式关闭时必须传递到运行时，避免转换层重新启用指标。
    pub engulfing_signal: Option<EngulfingSignalConfig>,
    /// EMA Touch 趋势配置；显式关闭时必须传递到运行时。
    pub ema_touch_trend_signal: Option<EmaTouchTrendSignalConfig>,
    /// legdetection信号；为空时使用默认值或表示不限制。
    pub leg_detection_signal: Option<LegDetectionConfig>,
    /// 市场structure信号；为空时使用默认值或表示不限制。
    pub market_structure_signal: Option<MarketStructureConfig>,
    /// rangefilter信号；为空时使用默认值或表示不限制。
    pub range_filter_signal: Option<RangeFilterConfig>,
    /// 配置项。
    pub chase_confirm_config: Option<ChaseConfirmConfig>,
    /// extremekfilter信号；为空时使用默认值或表示不限制。
    pub extreme_k_filter_signal: Option<ExtremeKFilterConfig>,
    /// 配置项。
    pub ema_distance_config: Option<EmaDistanceConfig>,
    /// ATR 止损倍数；为空时使用默认倍数。
    pub atr_stop_loss_multiplier: Option<f64>,
    /// 是否输出调试信息；为空时使用默认值。
    pub emit_debug: Option<bool>,
    /// macd信号；为空时使用默认值或表示不限制。
    pub macd_signal: Option<MacdSignalConfig>,
    /// fibretracement信号；为空时使用默认值或表示不限制。
    pub fib_retracement_signal: Option<FibRetracementSignalConfig>,
    /// 配置项。
    pub entry_block_config: Option<EntryBlockConfig>,
    /// 4H 动量激活代理；为空时保持默认关闭。
    pub candle_momentum_activation: Option<CandleMomentumActivationConfig>,
    /// 跨币种 ATR 与成交量分位数阈值；为空时保持默认关闭。
    pub cross_asset_adaptive_threshold: Option<CrossAssetAdaptiveThresholdConfig>,
    /// 两根 K 线流动性扫单反转；为空时保持默认关闭。
    pub liquidity_sweep_reversal: Option<LiquiditySweepReversalConfig>,
    /// 窄幅整理后的放量实体突破；为空时保持默认关闭。
    pub compressed_range_breakout: Option<CompressedRangeBreakoutConfig>,
    /// EMA144/169 隧道顺势回踩确认；为空时保持默认关闭。
    pub ema_tunnel_retest_confirmation: Option<EmaTunnelRetestConfirmationConfig>,
    /// 固定历史成交量价值区突破回踩；为空时保持默认关闭。
    pub volume_profile_value_area_retest: Option<VolumeProfileValueAreaRetestConfig>,
    /// 固定历史成交量价值区即时突破；为空时保持默认关闭。
    pub volume_profile_value_area_breakout: Option<VolumeProfileValueAreaBreakoutConfig>,
    /// 固定历史价值区上方失败拍卖做空；为空时保持默认关闭。
    pub volume_profile_failed_auction: Option<VolumeProfileFailedAuctionConfig>,
    /// Donchian 20 根放量通道突破；为空时保持默认关闭。
    pub donchian_volume_breakout: Option<DonchianVolumeBreakoutConfig>,
    /// Donchian 突破后紧邻一棒接受；为空时保持默认关闭。
    pub donchian_breakout_acceptance: Option<DonchianBreakoutAcceptanceConfig>,
    /// bearish BOS 环境中的 FVG 首次回补失败；为空时保持默认关闭。
    pub bos_fvg_retest: Option<BosFvgRetestConfig>,
    /// bearish FVG 完整收复多头；为空时保持默认关闭。
    pub fvg_reclaim: Option<FvgReclaimConfig>,
    /// MACD 背离经 fresh internal CHoCH 确认的反转；为空时保持默认关闭。
    pub macd_divergence_reversal: Option<MacdDivergenceReversalConfig>,
    /// MACD 趋势侧复位 + fresh internal BOS；为空时保持默认关闭。
    pub macd_trend_reset_bos: Option<MacdTrendResetBosConfig>,
    /// 空头 `1.5R` 盈利保护；为空时保持默认关闭。
    pub short_profit_protection: Option<ShortProfitProtectionConfig>,
}
impl ParamMergeBuilder {
    //使用构造器
    pub fn build() -> Self {
        Self::default()
    }
    /// 提供bb周期的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn bb_periods(mut self, bb_period: i32) -> Self {
        self.bb_period = bb_period;
        self
    }
    /// 提供hammershadowratio的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn hammer_shadow_ratio(mut self, shadow_ratio: f64) -> Self {
        self.hammer_shadow_ratio = shadow_ratio;
        self
    }
    /// 提供bbmultiplier的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn bb_multiplier(mut self, bb_multiplier: f64) -> Self {
        self.bb_multiplier = bb_multiplier;
        self
    }
    /// 封装成交量barnum，减少配置运行时调用方重复实现相同细节。
    pub fn volume_bar_num(mut self, volume_bar_num: usize) -> Self {
        self.volume_bar_num = volume_bar_num;
        self
    }
    /// 封装成交量increaseratio，减少配置运行时调用方重复实现相同细节。
    pub fn volume_increase_ratio(mut self, volume_increase_ratio: f64) -> Self {
        self.volume_increase_ratio = volume_increase_ratio;
        self
    }
    /// 封装成交量decreaseratio，减少配置运行时调用方重复实现相同细节。
    pub fn volume_decrease_ratio(mut self, volume_decrease_ratio: f64) -> Self {
        self.volume_decrease_ratio = volume_decrease_ratio;
        self
    }
    /// 提供breakthroughthreshold的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn breakthrough_threshold(mut self, breakthrough_threshold: f64) -> Self {
        self.breakthrough_threshold = breakthrough_threshold;
        self
    }
    /// 提供RSIperiod的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn rsi_period(mut self, rsi_period: usize) -> Self {
        self.rsi_period = rsi_period;
        self
    }
    /// 提供RSIoverbought的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn rsi_overbought(mut self, rsi_overbought: f64) -> Self {
        self.rsi_overbought = rsi_overbought;
        self
    }
    /// 提供RSIoversold的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn rsi_oversold(mut self, rsi_oversold: f64) -> Self {
        self.rsi_oversold = rsi_oversold;
        self
    }
    /// 提供K 线start时间的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn kline_start_time(mut self, kline_start_time: i64) -> Self {
        self.kline_start_time = Some(kline_start_time);
        self
    }
    /// 提供K 线end时间的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn kline_end_time(mut self, kline_end_time: i64) -> Self {
        self.kline_end_time = Some(kline_end_time);
        self
    }
    /// 计算最大losspercent，并把公式边界留在配置运行时内部。
    pub fn max_loss_percent(mut self, max_loss_percent: f64) -> Self {
        self.max_loss_percent = max_loss_percent;
        self
    }
    /// 提供take盈利ratio的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn take_profit_ratio(mut self, take_profit_ratio: f64) -> Self {
        self.take_profit_ratio = take_profit_ratio;
        self
    }
    /// 判断 配置、基础设施和运行时 条件是否满足，给上层流程提供布尔决策。
    pub fn is_used_signal_k_line_stop_loss(
        mut self,
        is_used_signal_k_line_stop_loss: bool,
    ) -> Self {
        self.is_used_signal_k_line_stop_loss = is_used_signal_k_line_stop_loss;
        self
    }
    /// 提供fix信号K 线take盈利ratio的集中实现，避免配置运行时调用方重复处理相同细节。
    pub fn fix_signal_kline_take_profit_ratio(
        mut self,
        fix_signal_kline_take_profit_ratio: f64,
    ) -> Self {
        self.fix_signal_kline_take_profit_ratio = Some(fix_signal_kline_take_profit_ratio);
        self
    }
    /// 转换为风控配置
    pub fn to_risk_config(&self) -> BasicRiskStrategyConfig {
        BasicRiskStrategyConfig {
            max_loss_percent: self.max_loss_percent,
            atr_take_profit_ratio: Some(self.take_profit_ratio),
            fixed_signal_kline_take_profit_ratio: self.fix_signal_kline_take_profit_ratio,
            is_used_signal_k_line_stop_loss: Some(self.is_used_signal_k_line_stop_loss),
            dynamic_max_loss: self.dynamic_max_loss.or(Some(true)),
            dynamic_entry_amp_threshold: self.dynamic_entry_amp_threshold,
            dynamic_entry_loss_percent: self.dynamic_entry_loss_percent,
            dynamic_entry_require_direction_mismatch: self.dynamic_entry_require_direction_mismatch,
            dynamic_range_threshold: self.dynamic_range_threshold,
            dynamic_range_loss_percent: self.dynamic_range_loss_percent,
            trade_fee_rate: None,
            position_leverage: self.position_leverage,
            tiered_take_profit_level_1_close_ratio: None,
            tiered_take_profit_level_2_close_ratio: None,
        }
    }
    /// 转换为 Vegas 策略配置
    pub fn to_vegas_strategy(&self, period: String) -> VegasStrategy {
        let volume_signal = VolumeSignalConfig {
            volume_bar_num: self.volume_bar_num,
            volume_increase_ratio: self.volume_increase_ratio,
            volume_decrease_ratio: self.volume_decrease_ratio,
            is_open: true,
        };
        let rsi_signal = RsiSignalConfig {
            rsi_length: self.rsi_period,
            rsi_oversold: self.rsi_oversold,
            rsi_overbought: self.rsi_overbought,
            is_open: true,
        };
        let kline_hammer_signal = KlineHammerConfig {
            up_shadow_ratio: self.hammer_shadow_ratio,
            down_shadow_ratio: self.hammer_shadow_ratio,
        };
        let signal_weights = self
            .signal_weights
            .clone()
            .or_else(|| Some(SignalWeightsConfig::default()));
        VegasStrategy {
            period,
            min_k_line_num: self.min_k_line_num.unwrap_or(3600),
            engulfing_signal: Some(self.engulfing_signal.unwrap_or_default()),
            ema_signal: Some(self.ema_signal.unwrap_or(EmaSignalConfig {
                ema_breakthrough_threshold: self.breakthrough_threshold,
                ..EmaSignalConfig::default()
            })),
            signal_weights,
            volume_signal: Some(volume_signal),
            candle_momentum_activation: self.candle_momentum_activation.unwrap_or_default(),
            cross_asset_adaptive_threshold: self.cross_asset_adaptive_threshold.unwrap_or_default(),
            ema_touch_trend_signal: Some(self.ema_touch_trend_signal.unwrap_or_default()),
            rsi_signal: Some(rsi_signal),
            bolling_signal: Some(BollingBandsSignalConfig {
                period: self.bb_period as usize,
                multiplier: self.bb_multiplier,
                is_open: true,
                consecutive_touch_times: 4,
            }),
            kline_hammer_signal: Some(kline_hammer_signal),
            leg_detection_signal: self.leg_detection_signal,
            market_structure_signal: self.market_structure_signal,
            range_filter_signal: self
                .range_filter_signal
                .or_else(|| Some(RangeFilterConfig::default())),
            extreme_k_filter_signal: self
                .extreme_k_filter_signal
                .or_else(default_extreme_k_filter),
            chase_confirm_config: self
                .chase_confirm_config
                .or_else(default_chase_confirm_config),
            macd_signal: self.macd_signal.or_else(default_macd_signal_config),
            fib_retracement_signal: self
                .fib_retracement_signal
                .or_else(default_fib_retracement_signal_config),
            entry_block_config: self.entry_block_config.unwrap_or_default(),
            liquidity_sweep_reversal: self.liquidity_sweep_reversal.unwrap_or_default(),
            compressed_range_breakout: self.compressed_range_breakout.unwrap_or_default(),
            ema_tunnel_retest_confirmation: self.ema_tunnel_retest_confirmation.unwrap_or_default(),
            volume_profile_value_area_retest: self
                .volume_profile_value_area_retest
                .unwrap_or_default(),
            volume_profile_value_area_breakout: self
                .volume_profile_value_area_breakout
                .unwrap_or_default(),
            volume_profile_failed_auction: self.volume_profile_failed_auction.unwrap_or_default(),
            donchian_volume_breakout: self.donchian_volume_breakout.unwrap_or_default(),
            donchian_breakout_acceptance: self.donchian_breakout_acceptance.unwrap_or_default(),
            bos_fvg_retest: self.bos_fvg_retest.unwrap_or_default(),
            fvg_reclaim: self.fvg_reclaim.unwrap_or_default(),
            macd_divergence_reversal: self.macd_divergence_reversal.unwrap_or_default(),
            macd_trend_reset_bos: self.macd_trend_reset_bos.unwrap_or_default(),
            short_profit_protection: self.short_profit_protection.unwrap_or_default(),
            ema_distance_config: self.ema_distance_config.unwrap_or_default(),
            atr_stop_loss_multiplier: self.atr_stop_loss_multiplier.unwrap_or(1.5),
            emit_debug: self.emit_debug.unwrap_or(true),
            large_entity_stop_loss_config: default_large_entity_stop_loss_config(),
        }
    }
}
//使用构造器
// 使用一个生成参数的函数，避免存储所有组合
pub struct ParamGenerator {
    /// 列表数据。
    bb_periods: Vec<i32>,
    /// 列表数据。
    hammer_shadow_ratios: Vec<f64>,
    /// 列表数据。
    bb_multipliers: Vec<f64>,
    /// 列表数据。
    volume_bar_nums: Vec<usize>,
    /// 列表数据。
    volume_ratios: Vec<f64>,
    /// 列表数据。
    breakthrough_thresholds: Vec<f64>,
    /// 列表数据。
    rsi_periods: Vec<usize>,
    /// 列表数据。
    rsi_over_buy_sell: Vec<(f64, f64)>,
    /// 当前索引。
    current_index: usize,
    /// 总数量。
    total_count: usize,
    /// 完整笛卡尔参数空间大小；采样索引必须始终映射到该范围内。
    grid_count: usize,
    /// 固定种子生成的首个网格索引。
    sample_start: usize,
    /// 与网格大小互质的步长，保证有限采样过程中不会重复命中同一组合。
    sample_stride: usize,
    //risk
    max_loss_percent: Vec<f64>,
    /// 列表数据。
    take_profit_ratios: Vec<f64>,
    /// isused信号kline止损亏损。
    is_used_signal_k_line_stop_loss: Vec<bool>,
    /// 列表数据。
    fix_signal_kline_take_profit_ratios: Vec<f64>,
}
impl ParamGenerator {
    #[allow(clippy::too_many_arguments)]
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    pub fn new(
        bb_periods: Vec<i32>,
        hammer_shadow_ratios: Vec<f64>,
        bb_multipliers: Vec<f64>,
        volume_bar_nums: Vec<usize>,
        volume_ratios: Vec<f64>,
        breakthrough_thresholds: Vec<f64>,
        rsi_periods: Vec<usize>,
        rsi_over_buy_sell: Vec<(f64, f64)>,
        max_loss_percent: Vec<f64>,
        take_profit_ratios: Vec<f64>,
        is_used_signal_k_line_stop_loss: Vec<bool>,
        fix_signal_kline_take_profit_ratios: Vec<f64>,
    ) -> Self {
        Self::new_sampled(
            bb_periods,
            hammer_shadow_ratios,
            bb_multipliers,
            volume_bar_nums,
            volume_ratios,
            breakthrough_thresholds,
            rsi_periods,
            rsi_over_buy_sell,
            max_loss_percent,
            take_profit_ratios,
            is_used_signal_k_line_stop_loss,
            fix_signal_kline_take_profit_ratios,
            usize::MAX,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// 构造可重放的有限随机采样器，不预先分配完整参数组合列表。
    pub fn new_sampled(
        bb_periods: Vec<i32>,
        hammer_shadow_ratios: Vec<f64>,
        bb_multipliers: Vec<f64>,
        volume_bar_nums: Vec<usize>,
        volume_ratios: Vec<f64>,
        breakthrough_thresholds: Vec<f64>,
        rsi_periods: Vec<usize>,
        rsi_over_buy_sell: Vec<(f64, f64)>,
        max_loss_percent: Vec<f64>,
        take_profit_ratios: Vec<f64>,
        is_used_signal_k_line_stop_loss: Vec<bool>,
        fix_signal_kline_take_profit_ratios: Vec<f64>,
        sample_size: usize,
        sample_seed: u64,
    ) -> Self {
        let grid_count = bb_periods.len()
            * hammer_shadow_ratios.len()
            * bb_multipliers.len()
            * volume_bar_nums.len()
            * volume_ratios.len()
            * breakthrough_thresholds.len()
            * rsi_periods.len()
            * rsi_over_buy_sell.len()
            * max_loss_percent.len()
            * take_profit_ratios.len()
            * is_used_signal_k_line_stop_loss.len()
            * fix_signal_kline_take_profit_ratios.len();
        let total_count = sample_size.min(grid_count);
        let (sample_start, sample_stride) = sample_order(grid_count, sample_size, sample_seed);
        Self {
            bb_periods,
            hammer_shadow_ratios,
            bb_multipliers,
            volume_bar_nums,
            volume_ratios,
            breakthrough_thresholds,
            rsi_periods,
            rsi_over_buy_sell,
            current_index: 0,
            total_count,
            grid_count,
            sample_start,
            sample_stride,
            max_loss_percent,
            take_profit_ratios,
            is_used_signal_k_line_stop_loss,
            fix_signal_kline_take_profit_ratios,
        }
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    pub fn get_next_batch(&mut self, batch_size: usize) -> Vec<ParamMergeBuilder> {
        let mut batch = Vec::with_capacity(batch_size);
        while batch.len() < batch_size && self.current_index < self.total_count {
            // 仿射排列只保存起点和步长，既能固定种子重放，又避免构造完整索引数组。
            let mut index = sampled_grid_index(
                self.current_index,
                self.grid_count,
                self.sample_start,
                self.sample_stride,
            );
            let bb_p_size = self.bb_periods.len();
            let sr_size = self.hammer_shadow_ratios.len();
            let bm_size = self.bb_multipliers.len();
            let vbn_size = self.volume_bar_nums.len();
            let vir_size = self.volume_ratios.len();
            let bt_size = self.breakthrough_thresholds.len();
            let rp_size = self.rsi_periods.len();
            let rob_size = self.rsi_over_buy_sell.len();
            let i_bb_p = index % bb_p_size;
            index /= bb_p_size;
            let i_sr = index % sr_size;
            index /= sr_size;
            let i_bm = index % bm_size;
            index /= bm_size;
            let i_vbn = index % vbn_size;
            index /= vbn_size;
            let i_vir = index % vir_size;
            index /= vir_size;
            let i_bt = index % bt_size;
            index /= bt_size;
            let i_rp = index % rp_size;
            index /= rp_size;
            let i_rob = index % rob_size;
            index /= rob_size;
            let i_mlp = index % self.max_loss_percent.len();
            index /= self.max_loss_percent.len();
            let i_pt = index % self.take_profit_ratios.len();
            index /= self.take_profit_ratios.len();
            let i_usklsl = index % self.is_used_signal_k_line_stop_loss.len();
            index /= self.is_used_signal_k_line_stop_loss.len();
            let i_fsktpr = index % self.fix_signal_kline_take_profit_ratios.len();
            let param = ParamMergeBuilder {
                bb_period: self.bb_periods[i_bb_p],
                hammer_shadow_ratio: self.hammer_shadow_ratios[i_sr],
                bb_multiplier: self.bb_multipliers[i_bm],
                volume_bar_num: self.volume_bar_nums[i_vbn],
                volume_increase_ratio: self.volume_ratios[i_vir],
                volume_decrease_ratio: self.volume_ratios[i_vir],
                breakthrough_threshold: self.breakthrough_thresholds[i_bt],
                ema_signal: None,
                rsi_period: self.rsi_periods[i_rp],
                rsi_overbought: self.rsi_over_buy_sell[i_rob].0,
                rsi_oversold: self.rsi_over_buy_sell[i_rob].1,
                kline_start_time: None,
                kline_end_time: None,
                min_k_line_num: None,
                max_loss_percent: self.max_loss_percent[i_mlp],
                take_profit_ratio: self.take_profit_ratios[i_pt],
                is_used_signal_k_line_stop_loss: self.is_used_signal_k_line_stop_loss[i_usklsl],
                fix_signal_kline_take_profit_ratio: Some(
                    self.fix_signal_kline_take_profit_ratios[i_fsktpr],
                ),
                dynamic_max_loss: None,
                dynamic_entry_amp_threshold: None,
                dynamic_entry_loss_percent: None,
                dynamic_entry_require_direction_mismatch: None,
                dynamic_range_threshold: None,
                dynamic_range_loss_percent: None,
                position_leverage: None,
                signal_weights: None,
                engulfing_signal: None,
                ema_touch_trend_signal: None,
                leg_detection_signal: None,
                market_structure_signal: None,
                range_filter_signal: None,
                chase_confirm_config: None,
                extreme_k_filter_signal: None,
                ema_distance_config: None,
                atr_stop_loss_multiplier: None,
                emit_debug: None,
                macd_signal: None,
                fib_retracement_signal: None,
                entry_block_config: None,
                candle_momentum_activation: None,
                cross_asset_adaptive_threshold: None,
                liquidity_sweep_reversal: None,
                compressed_range_breakout: None,
                ema_tunnel_retest_confirmation: None,
                volume_profile_value_area_retest: None,
                volume_profile_value_area_breakout: None,
                volume_profile_failed_auction: None,
                donchian_volume_breakout: None,
                donchian_breakout_acceptance: None,
                bos_fvg_retest: None,
                fvg_reclaim: None,
                macd_divergence_reversal: None,
                macd_trend_reset_bos: None,
                short_profit_protection: None,
            };
            batch.push(param);
            self.current_index += 1;
        }
        batch
    }
    pub fn progress(&self) -> (usize, usize) {
        (self.current_index, self.total_count)
    }
    pub fn set_current_index(&mut self, index: usize) {
        self.current_index = index.min(self.total_count);
    }
    pub fn reset(&mut self) {
        self.current_index = 0;
    }
    pub fn is_completed(&self) -> bool {
        self.current_index >= self.total_count
    }
    pub fn remaining_count(&self) -> usize {
        self.total_count.saturating_sub(self.current_index)
    }
}

/// 生成无需额外内存的确定性采样顺序。
///
/// 步长必须与网格大小互质，否则仿射序列会在覆盖完整参数空间前提前循环。
fn sample_order(grid_count: usize, sample_size: usize, sample_seed: u64) -> (usize, usize) {
    if grid_count <= 1 {
        return (0, 1);
    }
    if sample_size >= grid_count && sample_seed == 0 {
        return (0, 1);
    }
    let sample_start = splitmix64(sample_seed) as usize % grid_count;
    let mut sample_stride =
        (splitmix64(sample_seed ^ 0x9E37_79B9_7F4A_7C15) as usize % grid_count).max(1);
    while greatest_common_divisor(sample_stride, grid_count) != 1 {
        sample_stride += 1;
        if sample_stride >= grid_count {
            sample_stride = 1;
        }
    }
    (sample_start, sample_stride)
}

fn sampled_grid_index(
    sample_index: usize,
    grid_count: usize,
    sample_start: usize,
    sample_stride: usize,
) -> usize {
    if grid_count == 0 {
        return 0;
    }
    ((sample_start as u128 + sample_index as u128 * sample_stride as u128) % grid_count as u128)
        as usize
}

fn greatest_common_divisor(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
#[cfg(test)]
mod tests {
    use super::{ParamGenerator, ParamMergeBuilder};
    use std::collections::HashSet;

    fn sampled_params(sample_size: usize, seed: u64) -> Vec<ParamMergeBuilder> {
        let mut generator = ParamGenerator::new_sampled(
            vec![10, 20],
            vec![0.6, 0.8],
            vec![2.0],
            vec![4],
            vec![1.8],
            vec![0.003],
            vec![8],
            vec![(70.0, 30.0), (80.0, 20.0)],
            vec![0.03, 0.05],
            vec![0.0],
            vec![true, false],
            vec![0.0],
            sample_size,
            seed,
        );
        let expected = sample_size.min(32);
        let params = generator.get_next_batch(expected.max(1));
        assert_eq!(generator.progress(), (params.len(), expected));
        params
    }

    fn param_identity(param: &ParamMergeBuilder) -> (i32, u64, u64, u64, bool) {
        (
            param.bb_period,
            param.hammer_shadow_ratio.to_bits(),
            param.rsi_overbought.to_bits(),
            param.max_loss_percent.to_bits(),
            param.is_used_signal_k_line_stop_loss,
        )
    }

    #[test]
    fn sampled_generator_is_deterministic_and_has_no_duplicates() {
        let first = sampled_params(16, 42);
        let replay = sampled_params(16, 42);
        let other_seed = sampled_params(16, 43);
        let first_ids: Vec<_> = first.iter().map(param_identity).collect();
        let replay_ids: Vec<_> = replay.iter().map(param_identity).collect();
        let other_ids: Vec<_> = other_seed.iter().map(param_identity).collect();
        assert_eq!(first_ids, replay_ids);
        assert_ne!(first_ids, other_ids);
        assert_eq!(first_ids.iter().copied().collect::<HashSet<_>>().len(), 16);
    }

    #[test]
    fn exhaustive_generator_preserves_rsi_pairs_and_all_risk_dimensions() {
        let params = sampled_params(usize::MAX, 0);
        let allowed_rsi_pairs = [(70.0, 30.0), (80.0, 20.0)];
        assert_eq!(params.len(), 32);
        assert!(params
            .iter()
            .all(|param| allowed_rsi_pairs.contains(&(param.rsi_overbought, param.rsi_oversold))));
        assert!(params.iter().any(|param| param.max_loss_percent == 0.03));
        assert!(params.iter().any(|param| param.max_loss_percent == 0.05));
        assert!(params
            .iter()
            .any(|param| param.is_used_signal_k_line_stop_loss));
        assert!(params
            .iter()
            .any(|param| !param.is_used_signal_k_line_stop_loss));
    }
    #[test]
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn to_risk_config_preserves_dynamic_max_loss_thresholds() {
        let params = ParamMergeBuilder {
            max_loss_percent: 0.05,
            take_profit_ratio: 5.0,
            is_used_signal_k_line_stop_loss: true,
            fix_signal_kline_take_profit_ratio: Some(0.25),
            dynamic_max_loss: Some(false),
            dynamic_entry_amp_threshold: Some(0.05),
            dynamic_entry_loss_percent: Some(0.03),
            dynamic_entry_require_direction_mismatch: Some(false),
            dynamic_range_threshold: Some(0.08),
            dynamic_range_loss_percent: Some(0.04),
            position_leverage: Some(0.6),
            ..Default::default()
        };
        let risk = params.to_risk_config();
        assert_eq!(risk.fixed_signal_kline_take_profit_ratio, Some(0.25));
        assert_eq!(risk.dynamic_max_loss, Some(false));
        assert_eq!(risk.dynamic_entry_amp_threshold, Some(0.05));
        assert_eq!(risk.dynamic_entry_loss_percent, Some(0.03));
        assert_eq!(risk.dynamic_entry_require_direction_mismatch, Some(false));
        assert_eq!(risk.dynamic_range_threshold, Some(0.08));
        assert_eq!(risk.dynamic_range_loss_percent, Some(0.04));
        assert_eq!(risk.position_leverage, Some(0.6));
    }
}
// ================================
// NWE 参数生成器（网格/顺序遍历）
// ================================
#[derive(Clone)]
pub struct NweParamGenerator {
    /// 列表数据。
    stc_fast_length: Vec<usize>,
    /// 列表数据。
    stc_slow_length: Vec<usize>,
    /// 列表数据。
    stc_cycle_length: Vec<usize>,
    /// 列表数据。
    stc_d1_length: Vec<usize>,
    /// 列表数据。
    stc_d2_length: Vec<usize>,
    /// 列表数据。
    rsi_periods: Vec<usize>,
    /// 列表数据。
    rsi_over_buy_sell: Vec<(f64, f64)>,
    /// 列表数据。
    k_line_hammer_shadow_ratios: Vec<f64>,
    /// 列表数据。
    atr_periods: Vec<usize>,
    /// 列表数据。
    atr_multipliers: Vec<f64>,
    /// 参与成交量计算的 K 线数量 列表。
    volume_bar_num: Vec<usize>,
    /// 列表数据。
    volume_ratios: Vec<f64>,
    /// 列表数据。
    nwe_periods: Vec<usize>,
    /// 列表数据。
    nwe_multi: Vec<f64>,
    // 风险参数空间
    max_loss_percent: Vec<f64>,
    /// 列表数据。
    take_profit_ratios: Vec<f64>,
    /// isused信号kline止损亏损。
    is_used_signal_k_line_stop_loss: Vec<bool>,
    /// 当前索引。
    current_index: usize,
    /// 总数量。
    total_count: usize,
}
impl NweParamGenerator {
    #[allow(clippy::too_many_arguments)]
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    pub fn new(
        stc_fast_length: Vec<usize>,
        stc_slow_length: Vec<usize>,
        stc_cycle_length: Vec<usize>,
        stc_d1_length: Vec<usize>,
        stc_d2_length: Vec<usize>,
        rsi_periods: Vec<usize>,
        rsi_over_buy_sell: Vec<(f64, f64)>,
        atr_periods: Vec<usize>,
        atr_multipliers: Vec<f64>,
        volume_bar_nums: Vec<usize>,
        volume_ratios: Vec<f64>,
        nwe_periods: Vec<usize>,
        nwe_multi: Vec<f64>,
        // 风险参数
        max_loss_percent: Vec<f64>,
        take_profit_ratios: Vec<f64>,
        is_used_signal_k_line_stop_loss: Vec<bool>,
        k_line_hammer_shadow_ratios: Vec<f64>,
    ) -> Self {
        let total_count = stc_fast_length.len()
            * stc_slow_length.len()
            * stc_cycle_length.len()
            * stc_d1_length.len()
            * stc_d2_length.len()
            * rsi_periods.len()
            * rsi_over_buy_sell.len()
            * atr_periods.len()
            * atr_multipliers.len()
            * volume_bar_nums.len()
            * volume_ratios.len()
            * nwe_periods.len()
            * nwe_multi.len()
            * max_loss_percent.len()
            * take_profit_ratios.len()
            * is_used_signal_k_line_stop_loss.len()
            * k_line_hammer_shadow_ratios.len();
        Self {
            stc_fast_length,
            stc_slow_length,
            stc_cycle_length,
            stc_d1_length,
            stc_d2_length,
            rsi_periods,
            rsi_over_buy_sell,
            k_line_hammer_shadow_ratios,
            atr_periods,
            atr_multipliers,
            volume_bar_num: volume_bar_nums,
            volume_ratios,
            nwe_periods,
            nwe_multi,
            max_loss_percent,
            take_profit_ratios,
            is_used_signal_k_line_stop_loss,
            current_index: 0,
            total_count,
        }
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    pub fn get_next_batch(
        &mut self,
        batch_size: usize,
    ) -> Vec<(
        rust_quant_strategies::implementations::nwe_strategy::NweStrategyConfig,
        rust_quant_strategies::strategy_common::BasicRiskStrategyConfig,
    )> {
        let mut batch = Vec::with_capacity(batch_size);
        while batch.len() < batch_size && self.current_index < self.total_count {
            let mut idx = self.current_index;
            let stc_fast_length_len = self.stc_fast_length.len();
            let stc_slow_length_len = self.stc_slow_length.len();
            let stc_cycle_length_len = self.stc_cycle_length.len();
            let stc_d1_length_len = self.stc_d1_length.len();
            let stc_d2_length_len = self.stc_d2_length.len();
            let rp_len = self.rsi_periods.len();
            let rob_len = self.rsi_over_buy_sell.len();
            let ap_len = self.atr_periods.len();
            let am_len = self.atr_multipliers.len();
            let vbn_len = self.volume_bar_num.len();
            let vr_len = self.volume_ratios.len();
            let nwe_p_len = self.nwe_periods.len();
            let nwe_m_len = self.nwe_multi.len();
            let mlp_len = self.max_loss_percent.len();
            let tpr_len = self.take_profit_ratios.len();
            let usklsl_len = self.is_used_signal_k_line_stop_loss.len();
            let kh_sr_len = self.k_line_hammer_shadow_ratios.len();
            // 按维度展开索引（顺序需与 total_count 维度相同）
            let i_stc_fast_length = idx % stc_fast_length_len;
            idx /= stc_fast_length_len;
            let i_stc_slow_length = idx % stc_slow_length_len;
            idx /= stc_slow_length_len;
            let i_stc_cycle_length = idx % stc_cycle_length_len;
            idx /= stc_cycle_length_len;
            let i_stc_d1_length = idx % stc_d1_length_len;
            idx /= stc_d1_length_len;
            let i_stc_d2_length = idx % stc_d2_length_len;
            idx /= stc_d2_length_len;
            let _i_rp = idx % rp_len;
            idx /= rp_len;
            let _i_rob = idx % rob_len;
            idx /= rob_len;
            let i_ap = idx % ap_len;
            idx /= ap_len;
            let i_am = idx % am_len;
            idx /= am_len;
            let i_vbn = idx % vbn_len;
            idx /= vbn_len;
            let i_vr = idx % vr_len;
            idx /= vr_len;
            let i_nwe_p = idx % nwe_p_len;
            idx /= nwe_p_len;
            let i_nwe_m = idx % nwe_m_len;
            idx /= nwe_m_len;
            let i_mlp = idx % mlp_len;
            idx /= mlp_len;
            let i_tpr = idx % tpr_len;
            idx /= tpr_len;
            let i_usklsl = idx % usklsl_len;
            idx /= usklsl_len;
            let i_kh_sr = idx % kh_sr_len;
            let mut cfg = rust_quant_strategies::implementations::nwe_strategy::NweStrategyConfig {
                period: "5m".to_string(),
                ..Default::default()
            };
            cfg.stc_fast_length = self.stc_fast_length[i_stc_fast_length];
            cfg.stc_slow_length = self.stc_slow_length[i_stc_slow_length];
            cfg.stc_cycle_length = self.stc_cycle_length[i_stc_cycle_length];
            cfg.stc_d1_length = self.stc_d1_length[i_stc_d1_length];
            cfg.stc_d2_length = self.stc_d2_length[i_stc_d2_length];
            cfg.atr_period = self.atr_periods[i_ap];
            cfg.atr_multiplier = self.atr_multipliers[i_am];
            cfg.nwe_period = self.nwe_periods[i_nwe_p];
            cfg.nwe_multi = self.nwe_multi[i_nwe_m];
            cfg.volume_bar_num = self.volume_bar_num[i_vbn];
            cfg.volume_ratio = self.volume_ratios[i_vr];
            // 使用 NWE 周期作为最小数据长度的基线，确保指标有足够数据
            cfg.min_k_line_num = 500;
            cfg.k_line_hammer_shadow_ratio = self.k_line_hammer_shadow_ratios[i_kh_sr];
            let risk = rust_quant_strategies::strategy_common::BasicRiskStrategyConfig {
                is_used_signal_k_line_stop_loss: Some(
                    self.is_used_signal_k_line_stop_loss[i_usklsl],
                ),
                max_loss_percent: self.max_loss_percent[i_mlp],
                atr_take_profit_ratio: Some(self.take_profit_ratios[i_tpr]),
                fixed_signal_kline_take_profit_ratio: None,
                dynamic_max_loss: Some(true),
                ..Default::default()
            };
            batch.push((cfg, risk));
            self.current_index += 1;
        }
        batch
    }
    pub fn progress(&self) -> (usize, usize) {
        (self.current_index, self.total_count)
    }
    pub fn set_current_index(&mut self, index: usize) {
        self.current_index = index.min(self.total_count);
    }
    pub fn reset(&mut self) {
        self.current_index = 0;
    }
    pub fn is_completed(&self) -> bool {
        self.current_index >= self.total_count
    }
    pub fn remaining_count(&self) -> usize {
        self.total_count.saturating_sub(self.current_index)
    }
}
