use hmac::digest::consts::U321;

#[derive(Default)]
pub struct ParamMerge {
    pub bb_period: i32,
    pub shadow_ratio: f64,
    pub bb_multiplier: f64,
    pub volume_bar_num: usize,
    pub volume_increase_ratio: f64,
    pub volume_decrease_ratio: f64,
    pub breakthrough_threshold: f64,
    pub rsi_period: usize,
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,

    pub kline_start_time:Option<i64>,
    pub kline_end_time:Option<i64>,
}
impl ParamMerge {
    //使用构造器
    pub fn build() -> Self {
        Self::default()
    }
    pub fn bb_periods(mut self,bb_period: i32) ->Self{
        self.bb_period=bb_period;
        self
    }
    pub fn shadow_ratio(mut self,shadow_ratio: f64) ->Self{
        self.shadow_ratio=shadow_ratio;
        self
    }
    pub fn bb_multiplier(mut self,bb_multiplier: f64) ->Self{
        self.bb_multiplier=bb_multiplier;
        self
    }
    pub fn volume_bar_num(mut self,volume_bar_num: usize) ->Self{
        self.volume_bar_num=volume_bar_num;
        self
    }
    pub fn volume_increase_ratio(mut self,volume_increase_ratio: f64) ->Self{
        self.volume_increase_ratio=volume_increase_ratio;
        self
    }
    pub fn volume_decrease_ratio(mut self,volume_decrease_ratio: f64) ->Self{
        self.volume_decrease_ratio=volume_decrease_ratio;
        self
    }
    pub fn breakthrough_threshold(mut self,breakthrough_threshold: f64) ->Self{
        self.breakthrough_threshold=breakthrough_threshold;
        self
    }
    pub fn rsi_period(mut self,rsi_period: usize) ->Self{
        self.rsi_period=rsi_period;
        self
    }
    pub fn rsi_overbought(mut self,rsi_overbought: f64) ->Self{
        self.rsi_overbought=rsi_overbought;
        self
    }
    pub fn rsi_oversold(mut self,rsi_oversold: f64) ->Self{
        self.rsi_oversold=rsi_oversold;
        self
    }
    pub fn kline_start_time(mut self,kline_start_time: i64) ->Self{
        self.kline_start_time=Some(kline_start_time);
        self
    }
    pub fn kline_end_time(mut self,kline_end_time: i64) ->Self{
        self.kline_end_time=Some(kline_end_time);
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
    volume_increase_ratios: Vec<f64>,
    volume_decrease_ratios: Vec<f64>,
    breakthrough_thresholds: Vec<f64>,
    rsi_periods: Vec<usize>,
    rsi_overboughts: Vec<f64>,
    rsi_oversolds: Vec<f64>,
    current_index: usize,
    total_count: usize,
}



impl ParamGenerator {
    pub fn new(
        bb_periods: Vec<i32>,
        shadow_ratios: Vec<f64>,
        bb_multipliers: Vec<f64>,
        volume_bar_nums: Vec<usize>,
        volume_increase_ratios: Vec<f64>,
        volume_decrease_ratios: Vec<f64>,
        breakthrough_thresholds: Vec<f64>,
        rsi_periods: Vec<usize>,
        rsi_overboughts: Vec<f64>,
        rsi_oversolds: Vec<f64>,
    ) -> Self {
        let total_count = bb_periods.len()
            * shadow_ratios.len()
            * bb_multipliers.len()
            * volume_bar_nums.len()
            * volume_increase_ratios.len()
            * volume_decrease_ratios.len()
            * breakthrough_thresholds.len()
            * rsi_periods.len()
            * rsi_overboughts.len()
            * rsi_oversolds.len();

        Self {
            bb_periods,
            shadow_ratios,
            bb_multipliers,
            volume_bar_nums,
            volume_increase_ratios,
            volume_decrease_ratios,
            breakthrough_thresholds,
            rsi_periods,
            rsi_overboughts,
            rsi_oversolds,
            current_index: 0,
            total_count,
        }
    }

    pub fn get_next_batch(
        &mut self,
        batch_size: usize,
    ) -> Vec<ParamMerge> {
        let mut batch = Vec::with_capacity(batch_size);

        // 计算当前组合的索引
        while batch.len() < batch_size && self.current_index < self.total_count {
            let mut index = self.current_index;

            // 计算每个维度的索引
            let bb_p_size = self.bb_periods.len();
            let sr_size = self.shadow_ratios.len();
            let bm_size = self.bb_multipliers.len();
            let vbn_size = self.volume_bar_nums.len();
            let vir_size = self.volume_increase_ratios.len();
            let vdr_size = self.volume_decrease_ratios.len();
            let bt_size = self.breakthrough_thresholds.len();
            let rp_size = self.rsi_periods.len();
            let rob_size = self.rsi_overboughts.len();

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

            let i_vdr = index % vdr_size;
            index /= vdr_size;

            let i_bt = index % bt_size;
            index /= bt_size;

            let i_rp = index % rp_size;
            index /= rp_size;

            let i_rob = index % rob_size;
            index /= rob_size;

            let i_ros = index % self.rsi_oversolds.len();

            // 获取参数值
            let param = ParamMerge {
                bb_period: self.bb_periods[i_bb_p],
                shadow_ratio: self.shadow_ratios[i_sr],
                bb_multiplier: self.bb_multipliers[i_bm],
                volume_bar_num: self.volume_bar_nums[i_vbn],
                volume_increase_ratio: self.volume_increase_ratios[i_vir],
                volume_decrease_ratio: self.volume_decrease_ratios[i_vdr],
                breakthrough_threshold: self.breakthrough_thresholds[i_bt],
                rsi_period: self.rsi_periods[i_rp],
                rsi_overbought: self.rsi_overboughts[i_rob],
                rsi_oversold: self.rsi_oversolds[i_ros],
                kline_start_time: None,
                kline_end_time: None,
            };

            batch.push(param);
            self.current_index += 1;
        }

        batch
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.current_index, self.total_count)
    }
}
