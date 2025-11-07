use rust_quant_indicators::vegas_indicator::EmaSignalValue;
use once_cell::OnceCell;
RBatis;
use rbdc_mysql::MysqlDriver;
use std::collections::HashMap;
use std::env;
use std::sync::Mutex;
use std::time::Duration;
use tracing::{error, info};

static EMA_INDICATOR_VALUES: OnceCell<Mutex<HashMap<String, EmaSignalValue>>> = OnceCell::new();

pub fn get_ema_indicator_values() -> &'static Mutex<HashMap<String, EmaSignalValue>> {
    EMA_INDICATOR_VALUES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_ema_indicator_values(inst_id_with_period: String, ema_signal_value: EmaSignalValue) {
    let mut ema_indicator_values = get_ema_indicator_values().lock().expect("获取锁失败");
    ema_indicator_values.insert(inst_id_with_period, ema_signal_value);
}

pub fn get_ema_indicator_values_by_inst_id_with_period(
    inst_id_with_period: String,
) -> Option<EmaSignalValue> {
    let ema_indicator_values = get_ema_indicator_values().lock().expect("获取锁失败");
    ema_indicator_values.get(&inst_id_with_period).cloned()
}
