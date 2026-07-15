use rust_quant_domain::Timeframe;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

/// Watchdog 允许恢复正常策略计算链路的最大延迟。
pub const WATCHDOG_TRIGGER_WINDOW_MS: i64 = 10_000;

/// 把生产使用的周期字符串转换为毫秒，供所有触发来源共用同一时效口径。
pub fn timeframe_duration_ms(timeframe: &str) -> Result<i64, String> {
    Timeframe::from_str(timeframe)
        .map(|value| value.to_minutes().saturating_mul(60_000))
        .map_err(|error| format!("不支持的 K 线周期 {timeframe}: {error}"))
}

/// Watchdog 对一根已确认 K 线的处理决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogDecision {
    /// K 线尚未到收盘边界。
    NotDue,
    /// 同一根或更新的 K 线已经处理。
    AlreadyHandled,
    /// 仍在十秒时效窗口内，可以进入幂等正常触发链路。
    Trigger,
    /// 已超过时效窗口，只允许记录漏触发，不得进入执行链路。
    Expired,
}

impl WatchdogDecision {
    /// 按 K 线开盘时间、周期和当前时间计算补触发决策。
    pub fn for_confirmed_candle(
        candle_open_ts: i64,
        timeframe_ms: i64,
        now_ms: i64,
        last_handled_candle_ts: Option<i64>,
    ) -> Self {
        if last_handled_candle_ts.is_some_and(|timestamp| timestamp >= candle_open_ts) {
            return Self::AlreadyHandled;
        }
        let close_boundary_ms = candle_open_ts.saturating_add(timeframe_ms);
        if now_ms < close_boundary_ms {
            return Self::NotDue;
        }
        if now_ms.saturating_sub(close_boundary_ms) <= WATCHDOG_TRIGGER_WINDOW_MS {
            Self::Trigger
        } else {
            Self::Expired
        }
    }
}

/// 单个 `symbol × timeframe` 的 WebSocket 与触发运行态快照。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CandleRuntimeSnapshot {
    /// 交易对。
    pub symbol: String,
    /// K 线周期。
    pub timeframe: String,
    /// 最后收到该业务 K 线消息的时间。
    pub last_message_at_ms: Option<i64>,
    /// 最后确认 K 线的开盘时间戳。
    pub last_confirmed_candle_ts: Option<i64>,
    /// 最后观察到确认 K 线的时间。
    pub last_confirmed_at_ms: Option<i64>,
    /// 最后成功进入策略回调的 K 线时间戳。
    pub last_triggered_candle_ts: Option<i64>,
    /// 最后成功进入策略回调的时间。
    pub last_triggered_at_ms: Option<i64>,
    /// 最后因超过十秒窗口而拒绝补触发的 K 线时间戳。
    pub last_expired_candle_ts: Option<i64>,
    /// 本次进程生命周期内记录的漏触发数量。
    pub missed_trigger_count: u64,
    /// 启动时 DB 中已有的最新确认 K 线；watchdog 不追溯处理该基线。
    pub startup_baseline_candle_ts: Option<i64>,
    /// 当前已原子 claim、正在触发或已经处理的最新 K 线。
    #[serde(skip)]
    claimed_candle_ts: Option<i64>,
}

/// 维护所有 `symbol × timeframe` 的进程内触发状态和幂等 claim。
#[derive(Default)]
pub struct CandleRuntimeRegistry {
    states: Mutex<HashMap<String, CandleRuntimeSnapshot>>,
}

impl CandleRuntimeRegistry {
    /// 生成稳定的订阅目标键。
    fn key(symbol: &str, timeframe: &str) -> String {
        format!("{}:{}", symbol, timeframe)
    }

    /// 获取或初始化一个订阅目标的可变运行态。
    fn state_mut<'a>(
        states: &'a mut HashMap<String, CandleRuntimeSnapshot>,
        symbol: &str,
        timeframe: &str,
    ) -> &'a mut CandleRuntimeSnapshot {
        states
            .entry(Self::key(symbol, timeframe))
            .or_insert_with(|| CandleRuntimeSnapshot {
                symbol: symbol.to_string(),
                timeframe: timeframe.to_string(),
                ..CandleRuntimeSnapshot::default()
            })
    }

    /// 预先登记订阅目标，使首次业务消息到达前也能出现在健康快照中。
    pub fn register_target(&self, symbol: &str, timeframe: &str) {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        Self::state_mut(&mut states, symbol, timeframe);
    }

    /// 初始化 watchdog 的 DB 基线，避免进程重启后把历史 K 线误报为本轮漏触发。
    pub fn seed_startup_baseline(&self, symbol: &str, timeframe: &str, candle_ts: i64) {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        let state = Self::state_mut(&mut states, symbol, timeframe);
        state.startup_baseline_candle_ts = Some(candle_ts);
        state.last_confirmed_candle_ts = Some(candle_ts);
    }

    /// 判断一根 K 线是否属于进程启动前已有的 DB 基线。
    pub fn is_at_or_before_startup_baseline(
        &self,
        symbol: &str,
        timeframe: &str,
        candle_ts: i64,
    ) -> bool {
        self.snapshot(symbol, timeframe)
            .and_then(|state| state.startup_baseline_candle_ts)
            .is_some_and(|baseline| candle_ts <= baseline)
    }

    /// 记录最后业务 K 线消息时间。
    pub fn record_message(&self, symbol: &str, timeframe: &str, observed_at_ms: i64) {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        Self::state_mut(&mut states, symbol, timeframe).last_message_at_ms = Some(observed_at_ms);
    }

    /// 记录最后确认 K 线及其观察时间。
    pub fn record_confirmed_candle(
        &self,
        symbol: &str,
        timeframe: &str,
        candle_ts: i64,
        observed_at_ms: i64,
    ) {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        let state = Self::state_mut(&mut states, symbol, timeframe);
        if state
            .last_confirmed_candle_ts
            .is_none_or(|timestamp| candle_ts > timestamp)
        {
            state.last_confirmed_candle_ts = Some(candle_ts);
            state.last_confirmed_at_ms = Some(observed_at_ms);
        } else if state.last_confirmed_candle_ts == Some(candle_ts)
            && state.last_confirmed_at_ms.is_none()
        {
            state.last_confirmed_at_ms = Some(observed_at_ms);
        }
    }

    /// 原子 claim 一根确认 K 线，保证 WS 与 watchdog 竞态时只进入一次回调。
    pub fn try_claim_trigger(&self, symbol: &str, timeframe: &str, candle_ts: i64) -> bool {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        let state = Self::state_mut(&mut states, symbol, timeframe);
        let already_handled = [
            state.claimed_candle_ts,
            state.last_triggered_candle_ts,
            state.last_expired_candle_ts,
        ]
        .into_iter()
        .flatten()
        .any(|timestamp| timestamp >= candle_ts);
        if already_handled {
            return false;
        }
        state.claimed_candle_ts = Some(candle_ts);
        true
    }

    /// 标记一根已 claim K 线已成功进入策略回调。
    pub fn record_trigger_success(
        &self,
        symbol: &str,
        timeframe: &str,
        candle_ts: i64,
        triggered_at_ms: i64,
    ) {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        let state = Self::state_mut(&mut states, symbol, timeframe);
        state.last_triggered_candle_ts = Some(candle_ts);
        state.last_triggered_at_ms = Some(triggered_at_ms);
    }

    /// 标记过期漏触发；该状态也参与幂等判断，防止后续重新进入策略回调。
    pub fn record_expired(&self, symbol: &str, timeframe: &str, candle_ts: i64) -> bool {
        let mut states = self
            .states
            .lock()
            .expect("candle runtime registry poisoned");
        let state = Self::state_mut(&mut states, symbol, timeframe);
        if state
            .last_expired_candle_ts
            .is_some_and(|timestamp| timestamp >= candle_ts)
        {
            return false;
        }
        state.last_expired_candle_ts = Some(candle_ts);
        state.claimed_candle_ts = Some(candle_ts);
        state.missed_trigger_count = state.missed_trigger_count.saturating_add(1);
        true
    }

    /// 返回参与 watchdog 幂等判断的最新 K 线时间戳。
    pub fn latest_handled_candle_ts(&self, symbol: &str, timeframe: &str) -> Option<i64> {
        self.snapshot(symbol, timeframe).and_then(|state| {
            [
                state.claimed_candle_ts,
                state.last_triggered_candle_ts,
                state.last_expired_candle_ts,
            ]
            .into_iter()
            .flatten()
            .max()
        })
    }

    /// 返回单个订阅目标的运行态快照。
    pub fn snapshot(&self, symbol: &str, timeframe: &str) -> Option<CandleRuntimeSnapshot> {
        self.states
            .lock()
            .expect("candle runtime registry poisoned")
            .get(&Self::key(symbol, timeframe))
            .cloned()
    }

    /// 返回所有订阅目标的运行态快照。
    pub fn snapshots(&self) -> Vec<CandleRuntimeSnapshot> {
        self.states
            .lock()
            .expect("candle runtime registry poisoned")
            .values()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{CandleRuntimeRegistry, WatchdogDecision, WATCHDOG_TRIGGER_WINDOW_MS};

    #[test]
    fn watchdog_allows_trigger_at_exact_ten_second_boundary() {
        assert_eq!(
            WatchdogDecision::for_confirmed_candle(1_000, 60_000, 71_000, None),
            WatchdogDecision::Trigger
        );
        assert_eq!(WATCHDOG_TRIGGER_WINDOW_MS, 10_000);
    }

    #[test]
    fn watchdog_expires_candle_after_ten_second_boundary() {
        assert_eq!(
            WatchdogDecision::for_confirmed_candle(1_000, 60_000, 71_001, None),
            WatchdogDecision::Expired
        );
    }

    #[test]
    fn trigger_claim_is_idempotent_per_symbol_and_timeframe() {
        let registry = CandleRuntimeRegistry::default();

        assert!(registry.try_claim_trigger("ETH-USDT-SWAP", "4H", 1_000));
        assert!(!registry.try_claim_trigger("ETH-USDT-SWAP", "4H", 1_000));
        assert!(registry.try_claim_trigger("ETH-USDT-SWAP", "4H", 2_000));
    }

    #[test]
    fn expired_candle_can_never_be_claimed_later() {
        let registry = CandleRuntimeRegistry::default();
        assert!(registry.record_expired("ETH-USDT-SWAP", "4H", 1_000));
        assert!(!registry.try_claim_trigger("ETH-USDT-SWAP", "4H", 1_000));
    }

    #[test]
    fn startup_baseline_blocks_historical_replay() {
        let registry = CandleRuntimeRegistry::default();
        registry.seed_startup_baseline("ETH-USDT-SWAP", "4H", 1_000);

        assert!(registry.is_at_or_before_startup_baseline("ETH-USDT-SWAP", "4H", 1_000));
        assert!(!registry.is_at_or_before_startup_baseline("ETH-USDT-SWAP", "4H", 2_000));
    }

    #[test]
    fn runtime_snapshot_tracks_message_confirmation_and_successful_trigger() {
        let registry = CandleRuntimeRegistry::default();
        registry.record_message("ETH-USDT-SWAP", "4H", 10_000);
        registry.record_confirmed_candle("ETH-USDT-SWAP", "4H", 1_000, 10_100);
        assert!(registry.try_claim_trigger("ETH-USDT-SWAP", "4H", 1_000));
        registry.record_trigger_success("ETH-USDT-SWAP", "4H", 1_000, 10_200);

        let snapshot = registry
            .snapshot("ETH-USDT-SWAP", "4H")
            .expect("runtime snapshot");
        assert_eq!(snapshot.last_message_at_ms, Some(10_000));
        assert_eq!(snapshot.last_confirmed_candle_ts, Some(1_000));
        assert_eq!(snapshot.last_confirmed_at_ms, Some(10_100));
        assert_eq!(snapshot.last_triggered_candle_ts, Some(1_000));
        assert_eq!(snapshot.last_triggered_at_ms, Some(10_200));
    }
}
