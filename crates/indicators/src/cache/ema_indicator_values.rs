// 从当前包引入 EmaSignalValue
use crate::vegas_indicator::EmaSignalValue;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::Mutex;
static EMA_INDICATOR_VALUES: OnceCell<Mutex<HashMap<String, EmaSignalValue>>> = OnceCell::new();
pub fn get_ema_indicator_values() -> &'static Mutex<HashMap<String, EmaSignalValue>> {
    EMA_INDICATOR_VALUES.get_or_init(|| Mutex::new(HashMap::new()))
}
/// 更新 回测与策略研究 状态，并保留调用方需要的结果或错误信息。
pub fn set_ema_indicator_values(inst_id_with_period: String, ema_signal_value: EmaSignalValue) {
    let mut ema_indicator_values = get_ema_indicator_values().lock().expect("获取锁失败");
    ema_indicator_values.insert(inst_id_with_period, ema_signal_value);
}
/// 加载 回测与策略研究 运行所需数据，并把缺失或异常交给调用方处理。
pub fn get_ema_indicator_values_by_inst_id_with_period(
    inst_id_with_period: String,
) -> Option<EmaSignalValue> {
    let ema_indicator_values = get_ema_indicator_values().lock().expect("获取锁失败");
    ema_indicator_values.get(&inst_id_with_period).cloned()
}
