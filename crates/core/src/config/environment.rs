use std::env;
/// 读取布尔型环境变量：支持 true/false/1/0（大小写不敏感）
/// 封装环境变量istrue，减少配置运行时调用方重复实现相同细节。
pub fn env_is_true(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => {
            let v = v.trim();
            v.eq_ignore_ascii_case("true") || v == "1"
        }
        Err(_) => default,
    }
}

/// 判断当前进程是否正在执行任一种随机参数回测。
///
/// 上层入口允许分别启用 Vegas 与 NWE；底层诊断和持久化必须使用同一口径，
/// 否则专用随机模式会误写逐笔成交、过滤信号和审计快照。
pub fn random_backtest_is_enabled() -> bool {
    [
        "ENABLE_RANDOM_TEST",
        "ENABLE_RANDOM_TEST_VEGAS",
        "ENABLE_RANDOM_TEST_NWE",
    ]
    .iter()
    .any(|key| env_is_true(key, false))
}
/// 读取字符串环境变量，若不存在则返回默认值
pub fn env_or_default(key: &str, default: &str) -> String {
    match env::var(key) {
        Ok(v) => v,
        Err(_) => default.to_string(),
    }
}
/// 读取 i64 环境变量，不存在或解析失败返回默认值
pub fn env_i64(key: &str, default: i64) -> i64 {
    match env::var(key) {
        Ok(v) => v.trim().parse::<i64>().ok().unwrap_or(default),
        Err(_) => default,
    }
}
/// K线缓存新鲜度（毫秒）优先级：
/// 1) CANDLE_CACHE_STALENESS_{PERIOD}_MS（如：CANDLE_CACHE_STALENESS_1H_MS）
/// 2) CANDLE_CACHE_STALENESS_MS（全局默认）
/// 3) 代码默认值
pub fn candle_cache_staleness_ms(period: &str, default_ms: i64) -> i64 {
    let sp_key = format!("CANDLE_CACHE_STALENESS_{}_MS", period.to_uppercase());
    let sp = env_i64(&sp_key, -1);
    if sp >= 0 {
        return sp;
    }
    env_i64("CANDLE_CACHE_STALENESS_MS", default_ms)
}
