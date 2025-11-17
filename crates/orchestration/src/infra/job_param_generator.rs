#[derive(Default, Clone)]
pub struct ParamMergeBuilder {
    //bolling
    pub bb_period: i32,
    pub bb_multiplier: f64,
    //volume
    pub volume_bar_num: usize,
    pub volume_increase_ratio: f64,
    pub volume_decrease_ratio: f64,
    pub breakthrough_threshold: f64,
    //rsi
    pub rsi_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,
    //hammer
    pub hammer_shadow_ratio: f64,
    //kline
    pub kline_start_time: Option<i64>,
    pub kline_end_time: Option<i64>,
    //risk
    pub max_loss_percent: f64,                          // 最大止损百分比
    pub take_profit_ratio: f64,                         // 盈利阈值，用于动态止盈
    pub is_move_stop_loss: bool, //是否使用移动止损,当盈利之后,止损价格变成开仓价
    pub is_used_signal_k_line_stop_loss: bool, //是否使用最低价止损,当价格低于入场k线的最低价时,止损。或者空单的时候,价格高于入场k线的最高价时,止损
    pub is_move_stop_open_price_when_touch_price: bool, //是否使用移动止损当达到一个特定的价格位置的时候，移动止损线到开仓价格附近
}
impl ParamMergeBuilder {
    //使用构造器
    pub fn build() -> Self {
        Self::default()
    }
    pub fn bb_periods(mut self, bb_period: i32) -> Self {
        self.bb_period = bb_period;
        self
    }
    pub fn hammer_shadow_ratio(mut self, shadow_ratio: f64) -> Self {
        self.hammer_shadow_ratio = shadow_ratio;
        self
    }
    pub fn bb_multiplier(mut self, bb_multiplier: f64) -> Self {
        self.bb_multiplier = bb_multiplier;
        self
    }
    pub fn volume_bar_num(mut self, volume_bar_num: usize) -> Self {
        self.volume_bar_num = volume_bar_num;
        self
    }
    pub fn volume_increase_ratio(mut self, volume_increase_ratio: f64) -> Self {
        self.volume_increase_ratio = volume_increase_ratio;
        self
    }
    pub fn volume_decrease_ratio(mut self, volume_decrease_ratio: f64) -> Self {
        self.volume_decrease_ratio = volume_decrease_ratio;
        self
    }
    pub fn breakthrough_threshold(mut self, breakthrough_threshold: f64) -> Self {
        self.breakthrough_threshold = breakthrough_threshold;
        self
    }
    pub fn rsi_period(mut self, rsi_period: usize) -> Self {
        self.rsi_period = rsi_period;
        self
    }
    pub fn rsi_overbought(mut self, rsi_overbought: f64) -> Self {
        self.rsi_overbought = rsi_overbought;
        self
    }
    pub fn rsi_oversold(mut self, rsi_oversold: f64) -> Self {
        self.rsi_oversold = rsi_oversold;
        self
    }
    pub fn kline_start_time(mut self, kline_start_time: i64) -> Self {
        self.kline_start_time = Some(kline_start_time);
        self
    }
    pub fn kline_end_time(mut self, kline_end_time: i64) -> Self {
        self.kline_end_time = Some(kline_end_time);
        self
    }

    pub fn max_loss_percent(mut self, max_loss_percent: f64) -> Self {
        self.max_loss_percent = max_loss_percent;
        self
    }
    pub fn take_profit_ratio(mut self, take_profit_ratio: f64) -> Self {
        self.take_profit_ratio = take_profit_ratio;
        self
    }
    pub fn is_move_stop_loss(mut self, is_move_stop_loss: bool) -> Self {
        self.is_move_stop_loss = is_move_stop_loss;
        self
    }
    pub fn is_used_signal_k_line_stop_loss(
        mut self,
        is_used_signal_k_line_stop_loss: bool,
    ) -> Self {
        self.is_used_signal_k_line_stop_loss = is_used_signal_k_line_stop_loss;
        self
    }
    pub fn is_move_stop_open_price_when_touch_price(
        mut self,
        is_move_stop_open_price_when_touch_price: bool,
    ) -> Self {
        self.is_move_stop_open_price_when_touch_price = is_move_stop_open_price_when_touch_price;
        self
    }
}
//使用构造器

// 使用一个生成参数的函数，避免存储所有组合
pub struct ParamGenerator {
    bb_periods: Vec<i32>,
    shadow_ratios: Vec<f64>,
    bb_multipliers: Vec<f64>,
    volume_bar_nums: Vec<usize>,
    volume_ratios: Vec<f64>,
    breakthrough_thresholds: Vec<f64>,
    rsi_periods: Vec<usize>,
    rsi_over_buy_sell: Vec<(f64, f64)>,
    current_index: usize,
    total_count: usize,
    //risk
    max_loss_percent: Vec<f64>,
    take_profit_ratios: Vec<f64>,
    is_move_stop_loss: Vec<bool>,
    is_used_signal_k_line_stop_loss: Vec<bool>,
    is_move_stop_open_price_when_touch_price: Vec<bool>,
}

impl ParamGenerator {
    pub fn new(
        bb_periods: Vec<i32>,
        shadow_ratios: Vec<f64>,
        bb_multipliers: Vec<f64>,
        volume_bar_nums: Vec<usize>,
        volume_ratios: Vec<f64>,
        breakthrough_thresholds: Vec<f64>,
        rsi_periods: Vec<usize>,
        rsi_over_buy_sell: Vec<(f64, f64)>,
        max_loss_percent: Vec<f64>,
        take_profit_ratios: Vec<f64>,
        is_move_stop_loss: Vec<bool>,
        is_used_signal_k_line_stop_loss: Vec<bool>,
        is_move_stop_open_price_when_touch_price: Vec<bool>,
    ) -> Self {
        let total_count = bb_periods.len()
            * shadow_ratios.len()
            * bb_multipliers.len()
            * volume_bar_nums.len()
            * volume_ratios.len()
            * breakthrough_thresholds.len()
            * rsi_periods.len()
            * rsi_over_buy_sell.len()
            * max_loss_percent.len()
            * take_profit_ratios.len()
            * is_move_stop_loss.len()
            * is_used_signal_k_line_stop_loss.len()
            * is_move_stop_open_price_when_touch_price.len();

        Self {
            bb_periods,
            shadow_ratios,
            bb_multipliers,
            volume_bar_nums,
            volume_ratios,
            breakthrough_thresholds,
            rsi_periods,
            rsi_over_buy_sell,
            current_index: 0,
            total_count,
            max_loss_percent,
            take_profit_ratios,
            is_move_stop_loss,
            is_used_signal_k_line_stop_loss,
            is_move_stop_open_price_when_touch_price,
        }
    }

    pub fn get_next_batch(&mut self, batch_size: usize) -> Vec<ParamMergeBuilder> {
        let mut batch = Vec::with_capacity(batch_size);

        // 计算当前组合的索引
        while batch.len() < batch_size && self.current_index < self.total_count {
            let mut index = self.current_index;

            // 计算每个维度的索引
            let bb_p_size = self.bb_periods.len();
            let sr_size = self.shadow_ratios.len();
            let bm_size = self.bb_multipliers.len();
            let vbn_size = self.volume_bar_nums.len();
            let vir_size = self.volume_ratios.len();
            let bt_size = self.breakthrough_thresholds.len();
            let rp_size = self.rsi_periods.len();
            let rob_size = self.rsi_over_buy_sell.len();

            let mlp_size = self.max_loss_percent.len();
            let pt_size = self.take_profit_ratios.len();
            let mst_size = self.is_move_stop_loss.len();
            let usklsl_size = self.is_used_signal_k_line_stop_loss.len();
            let mstoptp_size = self.is_move_stop_open_price_when_touch_price.len();
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

            let i_ros = index % self.rsi_over_buy_sell.len();
            index /= self.rsi_over_buy_sell.len();

            let i_mlp = index % self.max_loss_percent.len();
            index /= self.max_loss_percent.len();

            let i_pt = index % self.take_profit_ratios.len();
            index /= self.take_profit_ratios.len();

            let i_mst = index % self.is_move_stop_loss.len();
            index /= self.is_move_stop_loss.len();

            let i_usklsl = index % self.is_used_signal_k_line_stop_loss.len();
            index /= self.is_used_signal_k_line_stop_loss.len();
            let i_mstoptp = index % self.is_move_stop_open_price_when_touch_price.len();
            index /= self.is_move_stop_open_price_when_touch_price.len();
            // 最后一个维度，无需再除
            // index /= self.is_used_signal_k_line_stop_loss.len();

            // 获取参数值
            let param = ParamMergeBuilder {
                bb_period: self.bb_periods[i_bb_p],
                hammer_shadow_ratio: self.shadow_ratios[i_sr],
                bb_multiplier: self.bb_multipliers[i_bm],
                volume_bar_num: self.volume_bar_nums[i_vbn],
                volume_increase_ratio: self.volume_ratios[i_vir],
                volume_decrease_ratio: self.volume_ratios[i_vir],
                breakthrough_threshold: self.breakthrough_thresholds[i_bt],
                rsi_period: self.rsi_periods[i_rp],
                rsi_overbought: self.rsi_over_buy_sell[i_rob].0,
                rsi_oversold: self.rsi_over_buy_sell[i_ros].1,
                kline_start_time: None,
                kline_end_time: None,
                max_loss_percent: self.max_loss_percent[i_mlp],
                take_profit_ratio: self.take_profit_ratios[i_pt],
                is_move_stop_loss: self.is_move_stop_loss[i_mst],
                is_used_signal_k_line_stop_loss: self.is_used_signal_k_line_stop_loss[i_usklsl],
                is_move_stop_open_price_when_touch_price: self
                    .is_move_stop_open_price_when_touch_price[i_mstoptp],
            };

            batch.push(param);
            self.current_index += 1;
        }

        batch
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.current_index, self.total_count)
    }

    /// 设置当前索引（用于断点续传）
    pub fn set_current_index(&mut self, index: usize) {
        self.current_index = index.min(self.total_count);
    }

    /// 重置到开始位置
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// 检查是否已完成所有组合
    pub fn is_completed(&self) -> bool {
        self.current_index >= self.total_count
    }

    /// 获取剩余组合数
    pub fn remaining_count(&self) -> usize {
        if self.current_index >= self.total_count {
            0
        } else {
            self.total_count - self.current_index
        }
    }
}

// ================================
// NWE 参数生成器（网格/顺序遍历）
// ================================
#[derive(Clone)]
pub struct NweParamGenerator {
    rsi_periods: Vec<usize>,
    rsi_over_buy_sell: Vec<(f64, f64)>,
    k_line_hammer_shadow_ratios: Vec<f64>,
    atr_periods: Vec<usize>,
    atr_multipliers: Vec<f64>,

    volume_bar_num: Vec<usize>,
    volume_ratios: Vec<f64>,

    nwe_periods: Vec<usize>,
    nwe_multi: Vec<f64>,

    // 风险参数空间
    max_loss_percent: Vec<f64>,
    take_profit_ratios: Vec<f64>,
    is_move_stop_loss: Vec<bool>,
    is_used_signal_k_line_stop_loss: Vec<bool>,
    is_move_stop_open_price_when_touch_price: Vec<bool>,

    current_index: usize,
    total_count: usize,
}

impl NweParamGenerator {
    pub fn new(
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
        is_move_stop_loss: Vec<bool>,
        is_used_signal_k_line_stop_loss: Vec<bool>,
        is_move_stop_open_price_when_touch_price: Vec<bool>,
        k_line_hammer_shadow_ratios: Vec<f64>,
    ) -> Self {
        let total_count = rsi_periods.len()
            * rsi_over_buy_sell.len()
            * atr_periods.len()
            * atr_multipliers.len()
            * volume_bar_nums.len()
            * volume_ratios.len()
            * nwe_periods.len()
            * nwe_multi.len()
            * max_loss_percent.len()
            * take_profit_ratios.len()
            * is_move_stop_loss.len()
            * is_used_signal_k_line_stop_loss.len()
            * is_move_stop_open_price_when_touch_price.len()
            * k_line_hammer_shadow_ratios.len();
        Self {
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
            is_move_stop_loss,
            is_used_signal_k_line_stop_loss,
            is_move_stop_open_price_when_touch_price,
            current_index: 0,
            total_count,
        }
    }

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
            let msl_len = self.is_move_stop_loss.len();
            let usklsl_len = self.is_used_signal_k_line_stop_loss.len();
            let mstoptp_len = self.is_move_stop_open_price_when_touch_price.len();
            let kh_sr_len = self.k_line_hammer_shadow_ratios.len();
            // 按维度展开索引（顺序需与 total_count 维度相同）
            let i_rp = idx % rp_len;
            idx /= rp_len;
            let i_rob = idx % rob_len;
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
            let i_msl = idx % msl_len;
            idx /= msl_len;
            let i_usklsl = idx % usklsl_len; // 最后一维无需再除
            idx /= usklsl_len;
            let i_mstoptp = idx % mstoptp_len;
            idx /= mstoptp_len;
            let i_kh_sr = idx % kh_sr_len;
            idx /= kh_sr_len;

            let cfg = rust_quant_strategies::implementations::nwe_strategy::NweStrategyConfig {
                period: "5m".to_string(),
                rsi_period: self.rsi_periods[i_rp],
                rsi_overbought: self.rsi_over_buy_sell[i_rob].0,
                rsi_oversold: self.rsi_over_buy_sell[i_rob].1,
                atr_period: self.atr_periods[i_ap],
                atr_multiplier: self.atr_multipliers[i_am],
                nwe_period: self.nwe_periods[i_nwe_p],
                nwe_multi: self.nwe_multi[i_nwe_m],
                volume_bar_num: self.volume_bar_num[i_vbn],
                volume_ratio: self.volume_ratios[i_vr],
                // 使用 NWE 周期作为最小数据长度的基线，确保指标有足够数据
                min_k_line_num: 500,
                k_line_hammer_shadow_ratio: self.k_line_hammer_shadow_ratios[i_kh_sr],
            };
            let risk = rust_quant_strategies::strategy_common::BasicRiskStrategyConfig {
                is_used_signal_k_line_stop_loss: self.is_used_signal_k_line_stop_loss[i_usklsl],
                max_loss_percent: self.max_loss_percent[i_mlp],
                take_profit_ratio: self.take_profit_ratios[i_tpr],
                is_one_k_line_diff_stop_loss: self.is_move_stop_loss[i_msl],
                is_move_stop_open_price_when_touch_price: self
                    .is_move_stop_open_price_when_touch_price[i_mstoptp],
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
