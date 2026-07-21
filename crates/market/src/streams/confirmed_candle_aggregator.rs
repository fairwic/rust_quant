use anyhow::{bail, Context, Result};
use okx::dto::market_dto::CandleOkxRespDto;
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};

/// 放量基线使用的已确认历史 K 线数量。
pub const VOLUME_LOOKBACK: usize = 20;
const ONE_MINUTE_MS: i64 = 60_000;

/// 由全市场 1m 确认流派生的固定周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregatedTimeframe {
    /// 1 分钟收盘周期，直接来自交易所确认流。
    M1,
    /// 5 分钟收盘周期，由连续 1m K 线派生。
    M5,
    /// 15 分钟收盘周期，由连续 1m K 线派生。
    M15,
    /// 4 小时收盘周期，由连续 1m K 线派生。
    H4,
}

impl AggregatedTimeframe {
    /// 返回与现有 K 线分表及策略配置一致的周期名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::H4 => "4h",
        }
    }

    /// 返回周期长度，单位为毫秒。
    pub const fn duration_ms(self) -> i64 {
        match self {
            Self::M1 => ONE_MINUTE_MS,
            Self::M5 => 5 * ONE_MINUTE_MS,
            Self::M15 => 15 * ONE_MINUTE_MS,
            Self::H4 => 4 * 60 * ONE_MINUTE_MS,
        }
    }

    /// 返回完整周期必须包含的连续 1m K 线数量。
    const fn expected_one_minute_count(self) -> usize {
        (self.duration_ms() / ONE_MINUTE_MS) as usize
    }
}

/// 已确认收盘的归一化 K 线。
///
/// 数值使用 Decimal，避免把 240 根 1m 成交量聚合为 4H 时引入可见的浮点累计误差。
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedCandle {
    /// K 线开盘时间，Unix 毫秒时间戳。
    pub open_time_ms: i64,
    /// 开盘价。
    pub open: Decimal,
    /// 最高价。
    pub high: Decimal,
    /// 最低价。
    pub low: Decimal,
    /// 收盘价。
    pub close: Decimal,
    /// OKX `vol`，永续合约场景下通常表示成交张数。
    pub volume_contracts: Decimal,
    /// OKX `volCcy`，用于同一交易对前后 K 线的放量比较。
    pub volume_base: Decimal,
    /// OKX `volCcyQuote`；为空或交易所未提供时为零。
    pub volume_quote: Decimal,
}

impl ConfirmedCandle {
    /// 将 OKX 已确认 K 线转换成 Market 内部模型。
    pub fn try_from_okx(value: &CandleOkxRespDto) -> Result<Self> {
        if value.confirm != "1" {
            bail!("only confirmed OKX candles can enter the close aggregator");
        }
        let candle = Self {
            open_time_ms: value
                .ts
                .parse::<i64>()
                .with_context(|| format!("invalid OKX candle ts: {}", value.ts))?,
            open: parse_decimal_field(&value.o, "open")?,
            high: parse_decimal_field(&value.h, "high")?,
            low: parse_decimal_field(&value.l, "low")?,
            close: parse_decimal_field(&value.c, "close")?,
            volume_contracts: parse_decimal_field(&value.v, "volume contracts")?,
            volume_base: parse_decimal_field(&value.vol_ccy, "volume base")?,
            volume_quote: parse_optional_decimal_field(&value.vol_ccy_quote, "volume quote")?,
        };
        candle.validate()?;
        Ok(candle)
    }

    /// 拒绝未对齐或不自洽的数据，避免错误 K 线污染后续滚动窗口。
    fn validate(&self) -> Result<()> {
        if self.open_time_ms < 0 || self.open_time_ms % ONE_MINUTE_MS != 0 {
            bail!(
                "confirmed 1m candle must align to a minute boundary: {}",
                self.open_time_ms
            );
        }
        if self.open <= Decimal::ZERO
            || self.high <= Decimal::ZERO
            || self.low <= Decimal::ZERO
            || self.close <= Decimal::ZERO
        {
            bail!("confirmed candle prices must be positive");
        }
        if self.high < self.open.max(self.close) || self.low > self.open.min(self.close) {
            bail!("confirmed candle OHLC values are inconsistent");
        }
        if self.volume_contracts < Decimal::ZERO
            || self.volume_base < Decimal::ZERO
            || self.volume_quote < Decimal::ZERO
        {
            bail!("confirmed candle volumes must be non-negative");
        }
        Ok(())
    }
}

/// 一根已确认 K 线及其周期。
#[derive(Debug, Clone, PartialEq)]
pub struct TimeframedCandle {
    /// K 线所属周期。
    pub timeframe: AggregatedTimeframe,
    /// 已确认 K 线。
    pub candle: ConfirmedCandle,
}

/// 当前 K 线相对前 20 根已确认 K 线的成交量观测。
#[derive(Debug, Clone, PartialEq)]
pub struct CandleVolumeObservation {
    /// OKX 交易对，例如 `BTC-USDT-SWAP`。
    pub symbol: String,
    /// 观测周期。
    pub timeframe: AggregatedTimeframe,
    /// 当前已确认 K 线。
    pub candle: ConfirmedCandle,
    /// 前 20 根 K 线 `volCcy` 的算术平均值。
    pub previous_average_volume: Decimal,
    /// `当前 volCcy / 前 20 根平均 volCcy`。
    pub volume_ratio: Decimal,
}

/// 一次 1m 收盘输入产生的所有 Market 更新。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CandleAggregationUpdate {
    /// 本次确认的 1m K 线及由它完成的高周期 K 线。
    pub closed_candles: Vec<TimeframedCandle>,
    /// 已具备完整前 20 根样本的成交量观测。
    pub volume_observations: Vec<CandleVolumeObservation>,
}

/// 输入流存在缺口时返回的恢复范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandleGap {
    /// 发生缺口的交易对。
    pub symbol: String,
    /// 下一根期望的 1m K 线开盘时间，Unix 毫秒时间戳。
    pub expected_open_time_ms: i64,
    /// 实际收到的 1m K 线开盘时间，Unix 毫秒时间戳。
    pub actual_open_time_ms: i64,
}

/// 固定保留最近 20 根成交量及其滚动总和，避免每次收盘重新遍历历史。
#[derive(Debug, Default)]
struct RollingVolumeWindow {
    /// 按收盘时间升序保存的最近成交量。
    values: VecDeque<Decimal>,
    /// `values` 的滚动总和，用于常数时间计算均值。
    total: Decimal,
}

impl RollingVolumeWindow {
    /// 用历史尾部最多 20 根成交量重建窗口。
    fn seed(&mut self, values: impl IntoIterator<Item = Decimal>) {
        self.values.clear();
        self.total = Decimal::ZERO;
        let values = values.into_iter().collect::<Vec<_>>();
        let tail_start = values.len().saturating_sub(VOLUME_LOOKBACK);
        for value in values.into_iter().skip(tail_start) {
            self.push(value);
        }
    }

    /// 先以过去 20 根计算基线，再写入当前成交量，防止当前值泄漏进比较基线。
    fn observe_then_push(&mut self, current: Decimal) -> Option<(Decimal, Decimal)> {
        let observation = if self.values.len() == VOLUME_LOOKBACK {
            let average = self.total / Decimal::from(VOLUME_LOOKBACK as u64);
            (average > Decimal::ZERO).then(|| (average, current / average))
        } else {
            None
        };
        self.push(current);
        observation
    }

    /// 追加一个已确认成交量，并保持固定窗口与滚动总和一致。
    fn push(&mut self, value: Decimal) {
        if self.values.len() == VOLUME_LOOKBACK {
            if let Some(removed) = self.values.pop_front() {
                self.total -= removed;
            }
        }
        self.values.push_back(value);
        self.total += value;
    }
}

/// 尚未收满的高周期 K 线；只允许连续 1m 输入推进。
#[derive(Debug, Clone)]
struct PartialCandle {
    /// 高周期桶起点，Unix 毫秒时间戳。
    bucket_start_ms: i64,
    /// 最近纳入的 1m 开盘时间，Unix 毫秒时间戳。
    last_open_time_ms: i64,
    /// 当前桶已纳入的连续 1m K 线数量。
    one_minute_count: usize,
    /// 当前桶的 OHLCV 聚合值。
    candle: ConfirmedCandle,
}

impl PartialCandle {
    /// 从桶内第一根 1m K 线初始化高周期聚合状态。
    fn new(timeframe: AggregatedTimeframe, candle: &ConfirmedCandle) -> Self {
        let bucket_start_ms = align_bucket(candle.open_time_ms, timeframe.duration_ms());
        let last_open_time_ms = candle.open_time_ms;
        let mut candle = candle.clone();
        candle.open_time_ms = bucket_start_ms;
        Self {
            bucket_start_ms,
            last_open_time_ms,
            one_minute_count: 1,
            candle,
        }
    }

    /// 合并下一根 1m K 线，仅在桶内样本完整且抵达预期末分钟时返回收盘 K 线。
    fn push(
        &mut self,
        timeframe: AggregatedTimeframe,
        one_minute: &ConfirmedCandle,
    ) -> Option<ConfirmedCandle> {
        let bucket_start_ms = align_bucket(one_minute.open_time_ms, timeframe.duration_ms());
        if bucket_start_ms != self.bucket_start_ms {
            *self = Self::new(timeframe, one_minute);
            return None;
        }

        self.candle.high = self.candle.high.max(one_minute.high);
        self.candle.low = self.candle.low.min(one_minute.low);
        self.candle.close = one_minute.close;
        self.candle.volume_contracts += one_minute.volume_contracts;
        self.candle.volume_base += one_minute.volume_base;
        self.candle.volume_quote += one_minute.volume_quote;
        self.last_open_time_ms = one_minute.open_time_ms;
        self.one_minute_count += 1;

        let expected_last = self
            .bucket_start_ms
            .saturating_add(timeframe.duration_ms() - ONE_MINUTE_MS);
        (self.one_minute_count == timeframe.expected_one_minute_count()
            && self.last_open_time_ms == expected_last)
            .then(|| self.candle.clone())
    }
}

/// 单个交易对的连续性、成交量窗口与高周期部分桶状态。
#[derive(Debug, Default)]
struct SymbolAggregationState {
    /// 最近已消费的 1m 开盘时间；`None` 表示尚未收到实时或预热数据。
    last_one_minute_open_time_ms: Option<i64>,
    /// 1m 最近 20 根成交量窗口。
    volume_1m: RollingVolumeWindow,
    /// 5m 最近 20 根成交量窗口。
    volume_5m: RollingVolumeWindow,
    /// 15m 最近 20 根成交量窗口。
    volume_15m: RollingVolumeWindow,
    /// 4H 最近 20 根成交量窗口。
    volume_4h: RollingVolumeWindow,
    /// 当前未收盘的 5m 桶。
    partial_5m: Option<PartialCandle>,
    /// 当前未收盘的 15m 桶。
    partial_15m: Option<PartialCandle>,
    /// 当前未收盘的 4H 桶。
    partial_4h: Option<PartialCandle>,
}

impl SymbolAggregationState {
    /// 返回指定周期的成交量窗口，统一预热与实时写入路径。
    fn volume_window_mut(&mut self, timeframe: AggregatedTimeframe) -> &mut RollingVolumeWindow {
        match timeframe {
            AggregatedTimeframe::M1 => &mut self.volume_1m,
            AggregatedTimeframe::M5 => &mut self.volume_5m,
            AggregatedTimeframe::M15 => &mut self.volume_15m,
            AggregatedTimeframe::H4 => &mut self.volume_4h,
        }
    }

    /// 返回高周期部分桶；1m 不经过桶聚合，因此调用 1m 属于编程错误。
    fn partial_mut(&mut self, timeframe: AggregatedTimeframe) -> &mut Option<PartialCandle> {
        match timeframe {
            AggregatedTimeframe::M5 => &mut self.partial_5m,
            AggregatedTimeframe::M15 => &mut self.partial_15m,
            AggregatedTimeframe::H4 => &mut self.partial_4h,
            AggregatedTimeframe::M1 => unreachable!("1m candles are not aggregated from buckets"),
        }
    }
}

/// 单任务持有的全市场 K 线聚合器。
///
/// 该类型不使用锁；调用方应让一个 Tokio task 顺序消费确认 K 线，从而同时保证时序和低延迟。
#[derive(Debug, Default)]
pub struct ConfirmedCandleAggregator {
    /// 按交易对隔离的内存状态，由单一消费任务顺序访问。
    states: HashMap<String, SymbolAggregationState>,
}

impl ConfirmedCandleAggregator {
    /// 用持久化的已确认 K 线预热某个周期的前 20 根成交量窗口。
    pub fn seed_volume_history(
        &mut self,
        symbol: &str,
        timeframe: AggregatedTimeframe,
        candles: &[ConfirmedCandle],
    ) {
        let state = self.states.entry(symbol.to_string()).or_default();
        state
            .volume_window_mut(timeframe)
            .seed(candles.iter().map(|candle| candle.volume_base));
    }

    /// 用当前 4H 桶内的连续 1m 历史恢复高周期部分聚合状态。
    pub fn seed_partial_one_minute_history(
        &mut self,
        symbol: &str,
        candles: &[ConfirmedCandle],
    ) -> Result<()> {
        let mut ordered = candles.to_vec();
        ordered.sort_unstable_by_key(|candle| candle.open_time_ms);
        let contiguous_start = ordered
            .windows(2)
            .rposition(|pair| pair[1].open_time_ms != pair[0].open_time_ms + ONE_MINUTE_MS)
            .map_or(0, |index| index + 1);
        let suffix = &ordered[contiguous_start..];
        let state = self.states.entry(symbol.to_string()).or_default();
        state.partial_5m = None;
        state.partial_15m = None;
        state.partial_4h = None;
        for candle in suffix {
            candle.validate()?;
            for timeframe in [
                AggregatedTimeframe::M5,
                AggregatedTimeframe::M15,
                AggregatedTimeframe::H4,
            ] {
                update_partial_without_emitting(state.partial_mut(timeframe), timeframe, candle);
            }
        }
        state.last_one_minute_open_time_ms = suffix.last().map(|candle| candle.open_time_ms);
        Ok(())
    }

    /// 顺序接收一根已确认 1m K 线，并同步完成所有内存聚合。
    pub fn ingest_one_minute(
        &mut self,
        symbol: &str,
        candle: ConfirmedCandle,
    ) -> std::result::Result<CandleAggregationUpdate, CandleGap> {
        let state = self.states.entry(symbol.to_string()).or_default();
        if let Some(last_open_time_ms) = state.last_one_minute_open_time_ms {
            if candle.open_time_ms <= last_open_time_ms {
                return Ok(CandleAggregationUpdate::default());
            }
            let expected_open_time_ms = last_open_time_ms + ONE_MINUTE_MS;
            if candle.open_time_ms != expected_open_time_ms {
                return Err(CandleGap {
                    symbol: symbol.to_string(),
                    expected_open_time_ms,
                    actual_open_time_ms: candle.open_time_ms,
                });
            }
        }

        state.last_one_minute_open_time_ms = Some(candle.open_time_ms);
        let mut update = CandleAggregationUpdate::default();
        record_closed_candle(
            symbol,
            AggregatedTimeframe::M1,
            candle.clone(),
            state,
            &mut update,
        );

        for timeframe in [
            AggregatedTimeframe::M5,
            AggregatedTimeframe::M15,
            AggregatedTimeframe::H4,
        ] {
            let completed = update_partial(state.partial_mut(timeframe), timeframe, &candle);
            if let Some(completed) = completed {
                record_closed_candle(symbol, timeframe, completed, state, &mut update);
            }
        }
        Ok(update)
    }
}

/// 推进指定高周期桶，并仅在该桶完整收盘时产出 K 线。
fn update_partial(
    partial: &mut Option<PartialCandle>,
    timeframe: AggregatedTimeframe,
    candle: &ConfirmedCandle,
) -> Option<ConfirmedCandle> {
    match partial {
        Some(partial) => partial.push(timeframe, candle),
        None => {
            *partial = Some(PartialCandle::new(timeframe, candle));
            None
        }
    }
}

/// 预热部分桶但不发出历史观测，避免启动时重放旧放量事件。
fn update_partial_without_emitting(
    partial: &mut Option<PartialCandle>,
    timeframe: AggregatedTimeframe,
    candle: &ConfirmedCandle,
) {
    let _ = update_partial(partial, timeframe, candle);
}

/// 先记录基于过去 20 根的放量观测，再把本根收盘加入滚动窗口。
fn record_closed_candle(
    symbol: &str,
    timeframe: AggregatedTimeframe,
    candle: ConfirmedCandle,
    state: &mut SymbolAggregationState,
    update: &mut CandleAggregationUpdate,
) {
    if let Some((previous_average_volume, volume_ratio)) = state
        .volume_window_mut(timeframe)
        .observe_then_push(candle.volume_base)
    {
        update.volume_observations.push(CandleVolumeObservation {
            symbol: symbol.to_string(),
            timeframe,
            candle: candle.clone(),
            previous_average_volume,
            volume_ratio,
        });
    }
    update
        .closed_candles
        .push(TimeframedCandle { timeframe, candle });
}

/// 将 1m 开盘时间对齐到对应高周期的 UTC 桶起点。
fn align_bucket(open_time_ms: i64, timeframe_ms: i64) -> i64 {
    open_time_ms - open_time_ms.rem_euclid(timeframe_ms)
}

/// 解析交易所必填十进制字段，并保留字段名帮助定位坏数据。
fn parse_decimal_field(value: &str, field: &str) -> Result<Decimal> {
    value
        .parse::<Decimal>()
        .with_context(|| format!("invalid OKX candle {field}: {value}"))
}

/// 解析交易所可选十进制字段；空字符串表示该交易所响应未提供该口径。
fn parse_optional_decimal_field(value: &str, field: &str) -> Result<Decimal> {
    if value.trim().is_empty() {
        return Ok(Decimal::ZERO);
    }
    parse_decimal_field(value, field)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(minute: i64, open: i64, close: i64, volume: i64) -> ConfirmedCandle {
        ConfirmedCandle {
            open_time_ms: minute * ONE_MINUTE_MS,
            open: Decimal::from(open),
            high: Decimal::from(open.max(close) + 1),
            low: Decimal::from(open.min(close) - 1),
            close: Decimal::from(close),
            volume_contracts: Decimal::from(volume),
            volume_base: Decimal::from(volume),
            volume_quote: Decimal::from(volume * close),
        }
    }

    #[test]
    fn aggregates_five_confirmed_minutes_into_one_exact_5m_candle() {
        let mut aggregator = ConfirmedCandleAggregator::default();
        let mut final_update = CandleAggregationUpdate::default();
        for minute in 0..5 {
            final_update = aggregator
                .ingest_one_minute(
                    "BTC-USDT-SWAP",
                    candle(minute, 100 + minute, 101 + minute, 10),
                )
                .expect("contiguous candle");
        }

        let five_minute = final_update
            .closed_candles
            .iter()
            .find(|item| item.timeframe == AggregatedTimeframe::M5)
            .expect("5m candle must close on minute four");
        assert_eq!(five_minute.candle.open_time_ms, 0);
        assert_eq!(five_minute.candle.open, Decimal::from(100));
        assert_eq!(five_minute.candle.close, Decimal::from(105));
        assert_eq!(five_minute.candle.high, Decimal::from(106));
        assert_eq!(five_minute.candle.low, Decimal::from(99));
        assert_eq!(five_minute.candle.volume_base, Decimal::from(50));
    }

    #[test]
    fn emits_ratio_only_after_twenty_previous_closed_candles() {
        let mut aggregator = ConfirmedCandleAggregator::default();
        for minute in 0..20 {
            let update = aggregator
                .ingest_one_minute("ETH-USDT-SWAP", candle(minute, 100, 100, 10))
                .expect("contiguous candle");
            assert!(update
                .volume_observations
                .iter()
                .all(|item| item.timeframe != AggregatedTimeframe::M1));
        }

        let update = aggregator
            .ingest_one_minute("ETH-USDT-SWAP", candle(20, 100, 100, 25))
            .expect("twenty-first candle");
        let observation = update
            .volume_observations
            .iter()
            .find(|item| item.timeframe == AggregatedTimeframe::M1)
            .expect("1m ratio");
        assert_eq!(observation.previous_average_volume, Decimal::from(10));
        assert_eq!(observation.volume_ratio, Decimal::new(25, 1));
    }

    #[test]
    fn completes_15m_and_4h_on_the_same_one_minute_input() {
        let mut aggregator = ConfirmedCandleAggregator::default();
        let mut final_update = CandleAggregationUpdate::default();
        for minute in 0..240 {
            final_update = aggregator
                .ingest_one_minute("SOL-USDT-SWAP", candle(minute, 100, 100, 1))
                .expect("contiguous candle");
        }

        assert!(final_update
            .closed_candles
            .iter()
            .any(|item| item.timeframe == AggregatedTimeframe::M15));
        let four_hour = final_update
            .closed_candles
            .iter()
            .find(|item| item.timeframe == AggregatedTimeframe::H4)
            .expect("4h close");
        assert_eq!(four_hour.candle.volume_base, Decimal::from(240));
    }

    #[test]
    fn duplicate_is_idempotent_and_gap_does_not_advance_state() {
        let mut aggregator = ConfirmedCandleAggregator::default();
        aggregator
            .ingest_one_minute("DOGE-USDT-SWAP", candle(0, 100, 100, 1))
            .expect("first candle");
        assert!(aggregator
            .ingest_one_minute("DOGE-USDT-SWAP", candle(0, 100, 100, 2))
            .expect("duplicate")
            .closed_candles
            .is_empty());

        let gap = aggregator
            .ingest_one_minute("DOGE-USDT-SWAP", candle(2, 100, 100, 1))
            .expect_err("minute one is missing");
        assert_eq!(gap.expected_open_time_ms, ONE_MINUTE_MS);
        aggregator
            .ingest_one_minute("DOGE-USDT-SWAP", candle(1, 100, 100, 1))
            .expect("repair candle");
        aggregator
            .ingest_one_minute("DOGE-USDT-SWAP", candle(2, 100, 100, 1))
            .expect("pending candle can be replayed after repair");
    }

    #[test]
    fn warmup_uses_only_contiguous_suffix_for_partial_buckets() {
        let mut aggregator = ConfirmedCandleAggregator::default();
        let history = vec![
            candle(0, 100, 100, 1),
            candle(1, 100, 100, 1),
            candle(3, 100, 100, 1),
            candle(4, 100, 100, 1),
        ];
        aggregator
            .seed_partial_one_minute_history("XRP-USDT-SWAP", &history)
            .expect("warmup");

        let update = aggregator
            .ingest_one_minute("XRP-USDT-SWAP", candle(5, 100, 100, 1))
            .expect("next minute");
        assert!(!update
            .closed_candles
            .iter()
            .any(|item| item.timeframe == AggregatedTimeframe::M5));
    }
}
