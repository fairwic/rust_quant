/// 直接从已完成 15m K 线判断的 v36 冻结 preset，不复用排名动量事件身份。
pub(crate) const MARKET_MOMENTUM_DIRECT_KLINE_V36_PRESET: &str =
    "research_market_momentum_direct_kline_reversal_long_15m_v36_frozen";
/// 直接 K 线 v36 的稳定规则版本，用于回测、明细与后续发布审计。
pub(crate) const MARKET_MOMENTUM_DIRECT_KLINE_V36_ENTRY_RULE_VERSION: &str =
    "kline15m_direct_momentum_reversal_long_v36_frozen";
/// 直接 K 线 v36 的独立策略身份，避免与排名动量反转策略混写。
pub(crate) const MARKET_MOMENTUM_DIRECT_KLINE_V36_STRATEGY_KEY: &str =
    "market_momentum_direct_kline_reversal";
/// 直接 K 线 v36 的独立产品 slug；当前只用于研究 manifest。
pub(crate) const MARKET_MOMENTUM_DIRECT_KLINE_V36_PRODUCT_SLUG: &str =
    "market-momentum-direct-kline-reversal";

/// RSI 放量/横盘突破 v1 的研究 preset；不注册到 paper observation。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V1_PRESET: &str =
    "research_market_rsi_volume_regime_both_15m_v1";
/// RSI 放量/横盘突破 v1 的稳定规则版本。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V1_ENTRY_RULE_VERSION: &str =
    "kline15m_rsi14_volume_regime_structure_stop_v1";
/// RSI 放量/布林压缩突破/因果背离 v2 的研究 preset；v1 继续保留用于结果复现。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V2_PRESET: &str =
    "research_market_rsi_volume_regime_bollinger_macd_divergence_both_15m_v2";
/// v2 使用独立规则版本，避免把新的横盘与背离语义混入 v1 回测明细。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V2_ENTRY_RULE_VERSION: &str =
    "kline15m_rsi14_volume_bollinger_macd_divergence_structure_stop_v2";
/// v3 固定四根量比、极值背离、压缩突破、96 根净幅与 ATR 风险语义，保留 v2 结果可重放。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V3_PRESET: &str =
    "research_market_rsi_volume_regime_atr_both_15m_v3";
/// v3 使用独立规则版本，禁止与已记录的 v2 研究证据混写。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V3_ENTRY_RULE_VERSION: &str =
    "kline15m_rsi_divergence_breakout_net8_atr15_v3";
/// v4 移除压缩突破，只保留 RSI 极值背离和 96 根净幅反转；v3 继续保持可重放。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V4_PRESET: &str =
    "research_market_rsi_volume_regime_divergence_net8_atr_both_15m_v4";
/// v4 使用独立规则版本，避免把分支删除混入已有 v3 回测证据。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V4_ENTRY_RULE_VERSION: &str =
    "kline15m_rsi_divergence_net8_atr15_no_sideways_breakout_v4";
/// v5 只调整成交量基线：用因果标记剔除最近十根中的历史放量，其他 v4 语义不变。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V5_PRESET: &str =
    "research_market_rsi_volume_regime_filtered_volume_divergence_net8_atr_both_15m_v5";
/// v5 使用独立规则版本，避免新的过滤均量覆盖 v4 的四根 1.5 倍量比证据。
pub(crate) const MARKET_RSI_VOLUME_REGIME_V5_ENTRY_RULE_VERSION: &str =
    "kline15m_rsi_divergence_net8_filtered_volume10_x2_atr15_v5";
/// RSI 放量反转策略族的独立身份，防止覆盖冻结 v36。
pub(crate) const MARKET_RSI_VOLUME_REGIME_STRATEGY_KEY: &str = "market_rsi_volume_regime";
/// RSI 放量反转策略族的研究产品 slug。
pub(crate) const MARKET_RSI_VOLUME_REGIME_PRODUCT_SLUG: &str = "market-rsi-volume-regime";

/// 过滤量比 + RSI/EMA/MACD 15m 策略的独立研究 preset，不继承 RSI volume regime 旧版本。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET: &str =
    "research_market_filtered_volume_rsi_ema_macd_both_15m_v1";
/// 新策略的不可变入场版本，供回测、明细和后续对照实验审计。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_volume3_rsi_ema_macd_atr15_v1";
/// v2 只修正 MACD 为已确认枢轴对背离；v1 继续保留用于历史回测复现。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET: &str =
    "research_market_filtered_volume_rsi_ema_macd_both_15m_v2";
/// v2 的稳定规则版本；未发布 Z 与 D_min 时只关闭 MACD 分支。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_volume3_rsi_ema_macd_pivot_atr15_v2";
/// v3 按最新规范增加同币种周 `vol_ccy` P90、RSI 吞没/枢轴语义和固定 ATR 距离止盈。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET: &str =
    "research_market_filtered_volume_weekly_base_volume_rsi_ema_macd_both_15m_v3";
/// v3 改变了基础成交量准入和风险契约，因此使用不可覆盖的新规则版本。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_volume3_weekly_base_volume_p90_rsi_ema_macd_structure_stop_fixed_atr_tp_v3";
/// v4 只在 v3 候选方向上叠加 BB(12, 2.6) 反向冲突缓冲，不允许布林带独立开仓。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET: &str = "research_market_filtered_volume_weekly_base_volume_bollinger_conflict_rsi_ema_macd_both_15m_v4";
/// v4 使用独立规则版本，避免布林冲突过滤覆盖已落库的 v3 研究证据。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION: &str = "kline15m_filtered_volume3_weekly_base_volume_p90_bollinger12x2p6_conflict_rsi_ema_macd_structure_stop_fixed_atr_tp_v4";
/// v5 只约束 EMA 延续分支离 EMA144 的 ATR 距离，不叠加 v4 布林冲突缓冲。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET: &str =
    "research_market_filtered_volume_weekly_base_volume_ema144_proximity_rsi_ema_macd_both_15m_v5";
/// v5 使用独立规则版本，避免 EMA144 距离门禁覆盖 v3/v4 已落库研究证据。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION: &str = "kline15m_filtered_volume3_weekly_base_volume_p90_ema144_distance_atr1_rsi_ema_macd_structure_stop_fixed_atr_tp_v5";
/// v6 只增加中性 RSI 长下影做多，用于隔离验证 DOGE 形态是否可跨币种泛化。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_PRESET: &str =
    "research_market_filtered_volume_neutral_rsi_lower_wick_long_both_15m_v6";
/// v6 不修改 v3 的成交量、其他入场分支与退出契约。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_volume3_weekly_p90_neutral_rsi_lower_wick_long_fixed_atr_tp_v6";
/// v7 保留 v3 入场，只研究目标完成比例盈利观察状态机。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_PRESET: &str =
    "research_market_filtered_volume_profit_observation_both_15m_v7";
/// v7 的退出语义与 v3 不同，必须使用独立且不可覆盖的版本。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_volume3_weekly_p90_v3_entry_target_completion_profit_observation_v7";
/// v8 组合 v6 入场与 v7 出场，只用于检查两项变化的交互。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_PRESET: &str =
    "research_market_filtered_volume_wick_profit_observation_both_15m_v8";
/// v8 不作为单项归因依据，仍保持 Research-only。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_volume3_weekly_p90_neutral_wick_target_completion_profit_observation_v8";
/// v9 用量比 2.5、周 P90 且 RSI 极值的放量 K 作为锚点和当前比较点。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET: &str =
    "research_market_filtered_volume2p5_weekly_p90_anchor_rsi_divergence_both_15m_v9";
/// v9 取消 RSI 背离右侧三根确认与一分改善，并冻结双端量比 2.5 门槛。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_weekly_p90_anchor_rsi_div_fixed_atr_tp_v9";
/// v9 改变了 RSI 背离入场语义，使用独立策略键避免与 v3 的严格枢轴结果混写。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_STRATEGY_KEY: &str =
    "market_filtered_volume_weekly_p90_anchor_rsi_divergence_15m_v1";
/// v9 仅用于研究，独立产品 slug 不注册 Paper/Live 消费入口。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRODUCT_SLUG: &str =
    "market-filtered-volume-weekly-p90-anchor-rsi-divergence-15m-v1";
/// v10 只保留 v9 锚点背离，并要求紧邻下一根已完成 K 线突破 p 的反转侧极值。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET: &str =
    "research_market_filtered_volume2p5_anchor_rsi_next_close_confirmed_both_15m_v10";
/// v10 的信号时间移到确认 K 完成时，随后仍在再下一根 15m 开盘成交。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_anchor_rsi_next_close_confirmed_fixed_atr_tp_v10";
/// v10 改变了入场时序，使用独立策略键，禁止覆盖 v9 的即时背离研究结果。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_STRATEGY_KEY: &str =
    "market_filtered_volume_anchor_rsi_next_close_15m_v1";
/// v10 保持 Research-only，不注册 Paper/Live 消费入口。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRODUCT_SLUG: &str =
    "market-filtered-volume-anchor-rsi-next-close-15m-v1";
/// v11 在 p 为方向性长影线时下一根开盘成交，否则只允许紧邻下一根盘中越过 p 高低点。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET: &str =
    "research_market_filtered_volume2p5_anchor_rsi_wick_or_next_touch_both_15m_v11";
/// v11 不等待下一根收盘，冻结影线直入与单根盘中触价两种因果成交语义。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_anchor_rsi_wick_or_next_touch_fixed_atr_tp_v11";
/// v11 改变成交时点和成交价，使用独立策略键，禁止覆盖 v9/v10 结果。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_STRATEGY_KEY: &str =
    "market_filtered_volume_anchor_rsi_wick_or_touch_15m_v1";
/// v11 保持 Research-only，不注册 Paper/Live 消费入口。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRODUCT_SLUG: &str =
    "market-filtered-volume-anchor-rsi-wick-or-touch-15m-v1";
/// v12 保持 v11 入场不变，只增加信号时点趋势止盈和持仓放量阶梯保护。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET: &str =
    "research_market_filtered_volume2p5_anchor_rsi_wick_or_next_touch_trend_exit_both_15m_v12";
/// v12 的退出和风险管理语义已经改变，必须使用不可覆盖的新规则版本。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_anchor_rsi_wick_or_touch_trend_tp_volume_trail_v12";
/// v12 使用独立策略键，避免趋势管理结果与 v11 固定目标结果混写。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_STRATEGY_KEY: &str =
    "market_filtered_volume_anchor_rsi_trend_managed_15m_v1";
/// v12 仍为 Research-only，不注册 Paper/Live 产品入口。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRODUCT_SLUG: &str =
    "market-filtered-volume-anchor-rsi-trend-managed-15m-v1";
/// v13 只把普通逆势目标从 1.0 ATR 提高到 1.5 ATR，其他 V12 契约保持不变。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET: &str =
    "research_market_filtered_volume2p5_anchor_rsi_wick_or_next_touch_trend_exit_counter15_both_15m_v13";
/// v13 使用独立规则版本，禁止把单变量结果覆盖进 V12 研究证据。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_anchor_rsi_wick_or_touch_trend_tp_counter15_volume_trail_v13";

/// 动量衰竭反转家族的独立研究预设，不继承 RSI、EMA 或平台分支。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET: &str =
    "research_market_momentum_exhaustion_reversal_both_15m_v1";
/// 动量衰竭反转家族的冻结规则版本。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_net96x8_wick_or_touch_fixed1r_v1";
/// 动量衰竭反转家族的独立策略键。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_STRATEGY_KEY: &str =
    "market_momentum_exhaustion_reversal_15m_v1";
/// 动量衰竭反转家族的 Research-only 产品标识。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRODUCT_SLUG: &str =
    "market-momentum-exhaustion-reversal-15m-v1";
/// 动量衰竭 V2 仅在方向性影线 setup 上启用 12 根限价等待，并恢复量比分档 ATR 目标。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET: &str =
    "research_market_momentum_exhaustion_reversal_both_15m_v2";
/// V2 的成交时序与退出距离均已改变，必须使用不可覆盖的新规则版本。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_net96x8_wick_limit12_volume_atr_tp_v2";
/// 动量衰竭 V2 使用独立策略键，禁止与固定 1R 的 V1 明细混写。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_STRATEGY_KEY: &str =
    "market_momentum_exhaustion_reversal_15m_v2";
/// 动量衰竭 V2 保持 Research-only，不注册 Paper/Live 消费入口。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRODUCT_SLUG: &str =
    "market-momentum-exhaustion-reversal-15m-v2";
/// 动量衰竭 V3 只把方向影线占振幅门槛从 60% 降到 55%，其余 V2 合同保持冻结。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET: &str =
    "research_market_momentum_exhaustion_reversal_both_15m_v3";
/// V3 使用不可覆盖的新规则版本，确保 55% 影线结果不会混入 V2 明细。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_net96x8_wick55_limit12_volume_atr_tp_v3";
/// 动量衰竭 V3 使用独立策略键，保持 Research-only 版本可审计。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_STRATEGY_KEY: &str =
    "market_momentum_exhaustion_reversal_15m_v3";
/// 动量衰竭 V3 的独立产品标识，不注册 Paper/Live。
pub(crate) const MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRODUCT_SLUG: &str =
    "market-momentum-exhaustion-reversal-15m-v3";

/// 放量锚点 RSI 背离家族的独立研究预设，不继承 EMA、平台或历史净移动分支。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET: &str =
    "research_market_volume_anchor_rsi_divergence_reversal_both_15m_v1";
/// 放量锚点 RSI 背离家族的冻结规则版本。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_anchor_rsi_wick_or_touch_fixed1r_v1";
/// 放量锚点 RSI 背离家族的独立策略键。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_STRATEGY_KEY: &str =
    "market_volume_anchor_rsi_divergence_reversal_15m_v1";
/// 放量锚点 RSI 背离家族的 Research-only 产品标识。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRODUCT_SLUG: &str =
    "market-volume-anchor-rsi-divergence-reversal-15m-v1";
/// 放量锚点 RSI V2 只增加四根间隔与 60/40 摆动重置门禁。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET: &str =
    "research_market_volume_anchor_rsi_divergence_reversal_both_15m_v2";
/// V2 使用不可覆盖的新规则版本，确保周期门禁结果不会混入 V1 明细。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_anchor_rsi_gap4_swing_reset_wick_or_touch_fixed1r_v2";
/// V2 使用独立策略键，供 V1/V2 回测按同口径直接对照。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_STRATEGY_KEY: &str =
    "market_volume_anchor_rsi_divergence_reversal_15m_v2";
/// V2 保持 Research-only，不注册 Paper/Live 消费入口。
pub(crate) const MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRODUCT_SLUG: &str =
    "market-volume-anchor-rsi-divergence-reversal-15m-v2";

/// 放量平台破位趋势家族的独立研究预设，不继承 RSI、MACD 或反转影线分支。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET: &str =
    "research_market_volume_platform_break_trend_both_15m_v1";
/// 放量平台破位趋势家族的冻结规则版本。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_platform20_confirm2_ema_trend_fixed1r_v1";
/// 放量平台破位趋势家族的独立策略键。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_STRATEGY_KEY: &str =
    "market_volume_platform_break_trend_15m_v1";
/// 放量平台破位趋势家族的 Research-only 产品标识。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRODUCT_SLUG: &str =
    "market-volume-platform-break-trend-15m-v1";
/// 平台趋势 V2 仅替换平台质量定义，保留 V1 的破位、确认、EMA 与风险合同。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET: &str =
    "research_market_volume_platform_break_trend_both_15m_v2";
/// V2 使用破位前 ATR、水平性和分散触碰门禁，不覆盖 V1 的宽松平台语义。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION: &str =
    "kline15m_filtered_vol2p5_horizontal_platform20_confirm2_ema_trend_fixed1r_v2";
/// 平台趋势 V2 使用独立策略键，确保回测明细可以按平台定义直接区分。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_STRATEGY_KEY: &str =
    "market_volume_platform_break_trend_15m_v2";
/// 平台趋势 V2 保持 Research-only，不注册 Paper/Live 消费入口。
pub(crate) const MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRODUCT_SLUG: &str =
    "market-volume-platform-break-trend-15m-v2";
/// 新策略使用独立 strategy key，避免与 V1-V5 研究结果混写。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_STRATEGY_KEY: &str =
    "market_filtered_volume_rsi_ema_macd_15m_v1";
/// 新策略的研究产品 slug；当前不注册 paper/live 消费入口。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRODUCT_SLUG: &str =
    "market-filtered-volume-rsi-ema-macd-15m-v1";
/// v3 的成交量与风险语义已改变，必须与 V1/V2 使用不同策略身份。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY: &str =
    "market_filtered_volume_weekly_base_volume_rsi_ema_macd_15m_v1";
/// v3 仍为研究产品，不注册 Paper/Live 消费入口。
pub(crate) const MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRODUCT_SLUG: &str =
    "market-filtered-volume-weekly-base-volume-rsi-ema-macd-15m-v1";

/// 判断是否使用周基础成交量、下一根开盘和 1% 风险定仓的 v3+ 研究契约。
pub(crate) fn is_filtered_volume_weekly_base_version(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION
    )
}

/// 判断是否为本轮拆分后的三个互斥入场策略家族。
pub(crate) fn is_isolated_entry_family_version(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION
    )
}

/// 返回独立入场家族的策略键；旧混合版本不在这里做隐式归类。
pub(crate) fn isolated_entry_family_strategy_key(entry_rule_version: &str) -> Option<&'static str> {
    match entry_rule_version {
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_STRATEGY_KEY)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_STRATEGY_KEY)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_STRATEGY_KEY)
        }
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_STRATEGY_KEY)
        }
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_STRATEGY_KEY)
        }
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_STRATEGY_KEY)
        }
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_STRATEGY_KEY)
        }
        _ => None,
    }
}

/// 返回独立入场家族的产品标识，供 manifest 与回测明细保持一致。
pub(crate) fn isolated_entry_family_product_slug(entry_rule_version: &str) -> Option<&'static str> {
    match entry_rule_version {
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRODUCT_SLUG)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRODUCT_SLUG)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRODUCT_SLUG)
        }
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRODUCT_SLUG)
        }
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRODUCT_SLUG)
        }
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRODUCT_SLUG)
        }
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRODUCT_SLUG)
        }
        _ => None,
    }
}

/// 判断是否采用锚点研究系列冻结的 2.5 倍过滤量比。
pub(crate) fn uses_filtered_volume_2p5(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION
    )
}

/// 判断是否启用只增加中性 RSI 长下影做多的入场消融。
pub(crate) fn uses_neutral_rsi_lower_wick_long(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION
    )
}

/// 判断是否启用目标完成比例盈利观察状态机。
pub(crate) fn uses_target_completion_profit_observation(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION
    )
}

/// 判断 RSI 背离是否改用“双端量比 2.5 + 周 P90 极值锚”的即时比较。
pub(crate) fn uses_weekly_p90_anchor_rsi_divergence(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
    )
}

/// 判断锚点背离是否必须由紧邻下一根完成 K 的收盘突破确认。
pub(crate) fn uses_anchor_next_close_confirmation(entry_rule_version: &str) -> bool {
    entry_rule_version == MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION
}

/// 判断是否采用 v11 的影线下一开盘或紧邻下一根盘中触价成交。
pub(crate) fn uses_anchor_wick_or_next_touch_entry(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION
            | MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION
    )
}

/// 判断是否启用动量衰竭 V2 的方向性影线限价等待与新 setup 替换。
pub(crate) fn uses_momentum_exhaustion_limit_entry(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
    )
}

/// 判断是否启用动量衰竭 V2+ 的 2.7/3.6/4.5 ATR 量比分档目标。
pub(crate) fn uses_momentum_exhaustion_volume_tier_exit(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION
            | MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION
    )
}

/// 判断是否启用 v12+ 的趋势目标选择和持仓放量阶梯保护。
pub(crate) fn uses_trend_managed_volume_trailing_exit(entry_rule_version: &str) -> bool {
    matches!(
        entry_rule_version,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION
            | MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION
    )
}

/// 返回 v3+ 研究版本的稳定 preset 名，避免空 CLI preset 被错误记录成 v3。
pub(crate) fn filtered_volume_weekly_base_preset(entry_rule_version: &str) -> Option<&'static str> {
    match entry_rule_version {
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V6_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V7_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V8_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET)
        }
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_ENTRY_RULE_VERSION => {
            Some(MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET)
        }
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_ENTRY_RULE_VERSION => {
            Some(MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET)
        }
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET)
        }
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET)
        }
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET)
        }
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_ENTRY_RULE_VERSION => {
            Some(MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET)
        }
        _ => None,
    }
}
