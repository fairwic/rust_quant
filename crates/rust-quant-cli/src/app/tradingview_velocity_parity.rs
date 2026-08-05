//! 冻结 TradingView Pine 的独立 Research-only 对照回放。
//!
//! 该模块只读取 OKX 公共历史行情，不注册策略目录、Paper、Live、调度器或数据库写入。

mod engine;
mod indicators;
mod model;
mod ranges;
mod signals;
pub mod strict_static_report;
pub mod strict_static_universe;
pub mod strict_static_universe_io;
mod strict_visual_breakout;
mod universe;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use okx::dto::market_dto::CandleOkxRespDto;
use reqwest::{Client, Proxy};
use std::time::Duration;

pub use engine::{
    replay, replay_with_anchor_upthrust_variant, replay_with_ema_short_variant,
    replay_with_ema_trend_long_variant, replay_with_sell_climax_base_reclaim_variant,
    replay_with_strict_visual_breakout_variant,
};
pub use indicators::compute_indicators;
pub use model::{
    AnchorUpthrustResearchVariant, BlockedSignal, Candle, Direction, EmaShortResearchVariant,
    EmaTrendLongResearchVariant, EntryIntent, ExitPolicy, ExitReason, HorizontalAnchorEvidence,
    Metrics, ParityRuleVersion, ReplayConfig, ReplayReport, SellClimaxBaseReclaimResearchVariant,
    SignalFamily, StrictVisualBreakoutResearchVariant, Trade, CURRENT_PINE_SOURCE_FNV1A32,
    CURRENT_STRATEGY_VERSION, PINE_SOURCE_FNV1A32, STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_BODY_MIDPOINT_HOLD_STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_BODY_STRENGTH_STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_CANDLE_EXTREME_STOP_STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_RETEST_ACCEPTANCE_STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_SHORT_RANGE_ONE_R_STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_STRATEGY_VERSION,
    STRICT_VISUAL_BREAKOUT_WEAK_DEPARTURE_PROBATION_STRATEGY_VERSION,
    STRICT_VISUAL_SYMMETRIC_RETAINED_BREAKOUT_STRATEGY_VERSION, V10_PINE_SOURCE_FNV1A32,
    V10_STRATEGY_VERSION, V11_PINE_SOURCE_FNV1A32, V11_STRATEGY_VERSION, V12_PINE_SOURCE_FNV1A32,
    V12_STRATEGY_VERSION, V13_PINE_SOURCE_FNV1A32, V13_STRATEGY_VERSION, V14_PINE_SOURCE_FNV1A32,
    V14_STRATEGY_VERSION, V15_PINE_SOURCE_FNV1A32, V15_STRATEGY_VERSION, V16_PINE_SOURCE_FNV1A32,
    V16_STRATEGY_VERSION, V17_PINE_SOURCE_FNV1A32, V17_STRATEGY_VERSION, V18_PINE_SOURCE_FNV1A32,
    V18_STRATEGY_VERSION, V19_PINE_SOURCE_FNV1A32, V19_STRATEGY_VERSION, V20_PINE_SOURCE_FNV1A32,
    V20_STRATEGY_VERSION, V21_STRATEGY_VERSION, V23_STRATEGY_VERSION, V24_STRATEGY_VERSION,
    V2_PINE_SOURCE_FNV1A32, V2_STRATEGY_VERSION, V4_PINE_SOURCE_FNV1A32, V4_STRATEGY_VERSION,
    V5_PINE_SOURCE_FNV1A32, V5_STRATEGY_VERSION, V6_PINE_SOURCE_FNV1A32, V6_STRATEGY_VERSION,
    V7_PINE_SOURCE_FNV1A32, V7_STRATEGY_VERSION, V8_PINE_SOURCE_FNV1A32, V8_STRATEGY_VERSION,
    V9_PINE_SOURCE_FNV1A32, V9_STRATEGY_VERSION,
};
pub use signals::volume_take_profit_atr;
pub use strict_static_report::{
    aggregate_metric_snapshot, assert_cost_path_parity, blocked_signal_count_in_window,
    blocked_signals_in_window, concentration_audit, event_cluster_audit, metric_snapshot,
    symbol_snapshot, ConcentrationAudit, EventClusterAudit, EventClusterSummary,
    RankedSymbolContribution, RankedTradeContribution, RemovalMetricSnapshot,
    SerializableAggregateSnapshot, SerializableMetricSnapshot, SerializableProfitFactor,
    SerializableSymbolSnapshot, STRICT_STATIC_EVENT_CLUSTER_MS, STRICT_STATIC_FEE_BPS_PER_SIDE,
    STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE,
};
pub use strict_visual_breakout::{
    strict_visual_breakout_body_strength, strict_visual_breakout_body_strength_for_variant,
    StrictVisualBreakoutBodyStrength, StrictVisualBreakoutSignal, StrictVisualConsolidationState,
    StrictVisualDepartureSide, StrictVisualLongEntryEvent, StrictVisualLongEntryState,
    StrictVisualRangeEvent, StrictVisualRangeEvidence, StrictVisualWeakDepartureEvidence,
    STRICT_VISUAL_BREAKOUT_MIN_BODY_RATIO, STRICT_VISUAL_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO,
    STRICT_VISUAL_RETAINED_ACCEPTANCE_WINDOW_BARS, STRICT_VISUAL_RETAINED_BREAKOUT_EXCESS_RATIO,
    STRICT_VISUAL_RETAINED_BREAKOUT_MIN_BODY_RATIO,
    STRICT_VISUAL_RETAINED_BREAKOUT_MIN_DIRECTIONAL_MOVE_RATIO,
    STRICT_VISUAL_SHORT_RANGE_ONE_R_MAX_BARS,
};
pub use universe::{
    load_frozen_top60_from_quant_core, load_frozen_universe_spec, FrozenSymbolCandles,
    FrozenUniverseData, FrozenUniverseSpec, SymbolCoverageDiagnostic, UniverseCoverageDiagnostic,
    FROZEN_UNIVERSE_MANIFEST_SHA256, FROZEN_UNIVERSE_VERSION, FROZEN_WARMUP_DAYS,
};

use super::market_velocity_backfill::fetch_okx_history_candles;

const OKX_REST_BASE: &str = "https://www.okx.com";
const TIMEFRAME: &str = "15m";
const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
// Rust V1 只承诺复现 66d3937e；当前 TradingView 主文件新增独立家族后必须显式另立 parity 版本。
const FROZEN_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_66d3937e.pine"
);
const V2_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_3cbbc9d8.pine"
);
const V3_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_7827654b.pine"
);
const V4_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_9ab73288.pine"
);
const V5_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_a36f0e19.pine"
);
const V6_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_60d9e838.pine"
);
const V7_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_aa8a1e37.pine"
);
const V8_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_252225ec.pine"
);
const V9_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_0e7a1393.pine"
);
const V10_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_06973f3c.pine"
);
const V11_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_53ba4291.pine"
);
const V12_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_34752685.pine"
);
const V13_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_b81e5d25.pine"
);
const V14_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_45391eac.pine"
);
const V15_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_range_squeeze_break_acceptance_v15_research.pine"
);
const V16_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_range_squeeze_right_side_trigger_v16_research.pine"
);
const V17_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_range_squeeze_right_side_trigger_ablation_v17_research.pine"
);
const V18_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_9f26295a.pine"
);
const V19_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_406cde87.pine"
);
const V20_PINE_SOURCE: &str = include_str!(
    "../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research_v20.pine"
);
const CURRENT_PINE_SOURCE: &str =
    include_str!("../../../../docs/strategy_list/15min_velocity_all_symbol_strategy_research.pine");

/// 从 OKX 公共只读接口加载已确认的现货 15m K 线。
pub async fn load_okx_spot_candles(
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
    proxy_url: Option<&str>,
) -> Result<Vec<Candle>> {
    if !matches!(symbol, "BTC-USDT" | "ETH-USDT") {
        bail!("parity research only supports frozen BTC-USDT and ETH-USDT cases");
    }
    if end_ms <= start_ms {
        bail!("end timestamp must be greater than start timestamp");
    }
    let client = build_read_only_client(proxy_url)?;
    let raw = fetch_okx_history_candles(
        &client,
        OKX_REST_BASE,
        symbol,
        TIMEFRAME,
        start_ms,
        end_ms,
        100,
        110,
    )
    .await
    .with_context(|| format!("fetch OKX spot candles for {symbol}"))?;
    let mut candles: Vec<Candle> = raw
        .into_iter()
        .filter(|candle| candle.confirm == "1")
        .map(parse_candle)
        .collect::<Result<_>>()?;
    candles.sort_by_key(|candle| candle.timestamp_ms);
    candles.dedup_by_key(|candle| candle.timestamp_ms);
    validate_candles(&candles)?;
    Ok(candles)
}

/// 把 RFC3339 时间转换为回放使用的 Unix 毫秒。
pub fn parse_timestamp(value: &str) -> Result<i64> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid RFC3339 timestamp: {value}"))
        .map(|timestamp| timestamp.with_timezone(&Utc).timestamp_millis())
}

/// 返回 BTC/ETH 现货的冻结 tick；若交易所合约规格改变，须作为新审计差异处理。
pub fn frozen_tick_size(symbol: &str) -> Result<f64> {
    match symbol {
        "BTC-USDT" => Ok(0.1),
        "ETH-USDT" => Ok(0.01),
        other => bail!("missing frozen tick size for {other}"),
    }
}

/// 在每次对照运行前验证本地 Pine 仍是冻结基线，避免“源码已变、标签未变”。
pub fn verify_frozen_pine_source() -> Result<()> {
    // 既有基线由 JavaScript `charCodeAt` 生成，因此这里按 UTF-16 code unit 复算，
    // 不能改成 UTF-8 字节哈希后仍沿用同一个身份。
    let actual = format!("{:08x}", fnv1a32_utf16(FROZEN_PINE_SOURCE));
    if actual != PINE_SOURCE_FNV1A32 {
        bail!(
            "Pine source drifted: expected {}, actual {}",
            PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V2 冻结快照，避免后续主 Pine 修改改变既有 V2 回放证据。
pub fn verify_v2_pine_source() -> Result<()> {
    let normalized = V2_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V2_PINE_SOURCE_FNV1A32 {
        bail!(
            "V2 Pine source drifted: expected {}, actual {}",
            V2_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V3 冻结快照，避免 V4 主 Pine 修改历史回放身份。
pub fn verify_v3_pine_source() -> Result<()> {
    let normalized = V3_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != CURRENT_PINE_SOURCE_FNV1A32 {
        bail!(
            "V3 Pine source drifted: expected {}, actual {}",
            CURRENT_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V4 冻结快照，避免 V5 主 Pine 修改历史回放身份。
pub fn verify_v4_pine_source() -> Result<()> {
    let normalized = V4_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V4_PINE_SOURCE_FNV1A32 {
        bail!(
            "V4 Pine source drifted: expected {}, actual {}",
            V4_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V5 冻结快照，避免未来主 Pine 迭代污染已生成的 V5 研究报告。
pub fn verify_v5_pine_source() -> Result<()> {
    let normalized = V5_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V5_PINE_SOURCE_FNV1A32 {
        bail!(
            "V5 Pine source drifted: expected {}, actual {}",
            V5_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V6 冻结快照，使候选回放不依赖后续主图选择或回滚动作。
pub fn verify_v6_pine_source() -> Result<()> {
    let normalized = V6_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V6_PINE_SOURCE_FNV1A32 {
        bail!(
            "V6 Pine source drifted: expected {}, actual {}",
            V6_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V7 冻结快照，使反向长影门禁不依赖后续主图选择。
pub fn verify_v7_pine_source() -> Result<()> {
    let normalized = V7_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V7_PINE_SOURCE_FNV1A32 {
        bail!(
            "V7 Pine source drifted: expected {}, actual {}",
            V7_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V8 冻结快照，使慢均线带门禁不依赖当前主图后续迭代。
pub fn verify_v8_pine_source() -> Result<()> {
    let normalized = V8_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V8_PINE_SOURCE_FNV1A32 {
        bail!(
            "V8 Pine source drifted: expected {}, actual {}",
            V8_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V9 冻结快照，使参数默认值调整不依赖当前主图后续迭代。
pub fn verify_v9_pine_source() -> Result<()> {
    let normalized = V9_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V9_PINE_SOURCE_FNV1A32 {
        bail!(
            "V9 Pine source drifted: expected {}, actual {}",
            V9_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V10 冻结快照，使五类入场质量门禁不依赖当前主图后续迭代。
pub fn verify_v10_pine_source() -> Result<()> {
    let normalized = V10_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V10_PINE_SOURCE_FNV1A32 {
        bail!(
            "V10 Pine source drifted: expected {}, actual {}",
            V10_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V11 冻结快照，使残差门禁不依赖当前主图后续迭代。
pub fn verify_v11_pine_source() -> Result<()> {
    let normalized = V11_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V11_PINE_SOURCE_FNV1A32 {
        bail!(
            "V11 Pine source drifted: expected {}, actual {}",
            V11_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V12 冻结快照，使去耦确认规则不依赖当前主图后续迭代。
pub fn verify_v12_pine_source() -> Result<()> {
    let normalized = V12_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V12_PINE_SOURCE_FNV1A32 {
        bail!(
            "V12 Pine source drifted: expected {}, actual {}",
            V12_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V13 冻结快照，使压缩扩张分阶段接受不依赖当前主图后续迭代。
pub fn verify_v13_pine_source() -> Result<()> {
    let normalized = V13_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V13_PINE_SOURCE_FNV1A32 {
        bail!(
            "V13 Pine source drifted: expected {}, actual {}",
            V13_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V14 冻结快照，使无方向压缩状态实验不依赖当前主图后续迭代。
pub fn verify_v14_pine_source() -> Result<()> {
    let normalized = V14_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V14_PINE_SOURCE_FNV1A32 {
        bail!(
            "V14 Pine source drifted: expected {}, actual {}",
            V14_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V15 独立 Pine，防止真实箱体 Research 结果与后续图表修改混淆。
pub fn verify_v15_pine_source() -> Result<()> {
    let normalized = V15_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V15_PINE_SOURCE_FNV1A32 {
        bail!(
            "V15 Pine source drifted: expected {}, actual {}",
            V15_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V16 独立 Pine，防止右侧触发结果与后续图表修改混淆。
pub fn verify_v16_pine_source() -> Result<()> {
    let normalized = V16_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V16_PINE_SOURCE_FNV1A32 {
        bail!(
            "V16 Pine source drifted: expected {}, actual {}",
            V16_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V17 独立 Pine，防止纯右侧触发消融结果与后续修改混淆。
pub fn verify_v17_pine_source() -> Result<()> {
    let normalized = V17_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V17_PINE_SOURCE_FNV1A32 {
        bail!(
            "V17 Pine source drifted: expected {}, actual {}",
            V17_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V18 组合主 Pine 的冻结源码身份。
pub fn verify_v18_pine_source() -> Result<()> {
    let normalized = V18_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V18_PINE_SOURCE_FNV1A32 {
        bail!(
            "V18 Pine source drifted: expected {}, actual {}",
            V18_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V19 假突破下影门禁主 Pine 的冻结源码身份。
pub fn verify_v19_pine_source() -> Result<()> {
    let normalized = V19_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V19_PINE_SOURCE_FNV1A32 {
        bail!(
            "V19 Pine source drifted: expected {}, actual {}",
            V19_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证 V20 早期失败接受空单主 Pine 的冻结源码身份。
pub fn verify_v20_pine_source() -> Result<()> {
    let normalized = V20_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V20_PINE_SOURCE_FNV1A32 {
        bail!(
            "V20 Pine source drifted: expected {}, actual {}",
            V20_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 验证当前 Research 主 Pine 已显式切换到可回滚的 V20 组合版本。
pub fn verify_current_pine_source() -> Result<()> {
    let normalized = CURRENT_PINE_SOURCE.replace("\r\n", "\n");
    let editor_source = normalized.strip_suffix('\n').unwrap_or(&normalized);
    let actual = format!("{:08x}", fnv1a32_utf16(editor_source));
    if actual != V20_PINE_SOURCE_FNV1A32 {
        bail!(
            "current V20 Research Pine source drifted: expected {}, actual {}",
            V20_PINE_SOURCE_FNV1A32,
            actual
        );
    }
    Ok(())
}

/// 把 OKX 字符串 OHLCV 转为回放模型；成交量固定取 `vol_ccy`，坏行立即失败而非跳过。
fn parse_candle(raw: CandleOkxRespDto) -> Result<Candle> {
    let candle = Candle {
        timestamp_ms: raw.ts.parse().context("parse OKX candle timestamp")?,
        open: raw.o.parse().context("parse OKX candle open")?,
        high: raw.h.parse().context("parse OKX candle high")?,
        low: raw.l.parse().context("parse OKX candle low")?,
        close: raw.c.parse().context("parse OKX candle close")?,
        volume: raw.vol_ccy.parse().context("parse OKX candle vol_ccy")?,
    };
    if !candle.is_valid() {
        bail!(
            "invalid confirmed OKX candle at {}: O={} H={} L={} C={} vol_ccy={}",
            candle.timestamp_ms,
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume
        );
    }
    Ok(candle)
}

/// 构造无隐式系统代理的只读客户端，使同一次 Research 回放的网络路径可审计。
fn build_read_only_client(proxy_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        // 该 Mac 的系统代理动态存储在无 GUI 会话中可能返回空对象；Research CLI
        // 必须显式选择直连或调用方给出的代理，不能依赖隐式系统状态。
        .no_proxy();
    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url).context("configure explicit proxy")?);
    }
    builder.build().context("build read-only OKX HTTP client")
}

/// 要求至少覆盖长周期指标预热，并保证输入严格落在连续 15m 时间网格。
fn validate_candles(candles: &[Candle]) -> Result<()> {
    if candles.len() < 700 {
        bail!(
            "parity replay requires at least 700 confirmed candles, got {}",
            candles.len()
        );
    }
    for pair in candles.windows(2) {
        let interval = pair[1].timestamp_ms - pair[0].timestamp_ms;
        if interval != CANDLE_INTERVAL_MS {
            bail!(
                "non-contiguous 15m history: {} -> {}, interval={}ms",
                pair[0].timestamp_ms,
                pair[1].timestamp_ms,
                interval
            );
        }
    }
    Ok(())
}

/// 按 JavaScript `charCodeAt` 的 UTF-16 code unit 语义计算 Pine 身份哈希。
fn fnv1a32_utf16(value: &str) -> u32 {
    value.encode_utf16().fold(0x811c9dc5_u32, |hash, unit| {
        (hash ^ u32::from(unit)).wrapping_mul(0x0100_0193)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_tick_is_explicit_per_spot_symbol() {
        assert_eq!(frozen_tick_size("BTC-USDT").unwrap(), 0.1);
        assert_eq!(frozen_tick_size("ETH-USDT").unwrap(), 0.01);
        assert!(frozen_tick_size("BTC-USDT-SWAP").is_err());
    }

    #[test]
    fn bundled_pine_source_matches_frozen_hash() {
        verify_frozen_pine_source().expect("Pine source must remain frozen");
    }

    #[test]
    fn frozen_v2_pine_source_matches_v2_hash() {
        verify_v2_pine_source().expect("frozen V2 Pine source must match V2");
    }

    #[test]
    fn current_pine_source_matches_v3_hash() {
        verify_v3_pine_source().expect("frozen Pine source must match V3");
    }

    #[test]
    fn frozen_v4_pine_source_matches_v4_hash() {
        verify_v4_pine_source().expect("frozen Pine source must match V4");
    }

    #[test]
    fn frozen_v5_through_v20_sources_match_and_current_is_v20() {
        verify_v5_pine_source().expect("frozen V5 Pine source must match V5");
        verify_v6_pine_source().expect("frozen V6 Pine source must match V6");
        verify_v7_pine_source().expect("frozen V7 Pine source must match V7");
        verify_v8_pine_source().expect("frozen V8 Pine source must match V8");
        verify_v9_pine_source().expect("frozen V9 Pine source must match V9");
        verify_v10_pine_source().expect("frozen V10 Pine source must match V10");
        verify_v11_pine_source().expect("frozen V11 Pine source must match V11");
        verify_v12_pine_source().expect("frozen V12 Pine source must match V12");
        verify_v13_pine_source().expect("frozen V13 Pine source must match V13");
        verify_v14_pine_source().expect("frozen V14 Pine source must match V14");
        verify_v15_pine_source().expect("frozen V15 Pine source must match V15");
        verify_v16_pine_source().expect("frozen V16 Pine source must match V16");
        verify_v17_pine_source().expect("frozen V17 Pine source must match V17");
        verify_v18_pine_source().expect("frozen V18 Pine source must match V18");
        verify_v19_pine_source().expect("frozen V19 Pine source must match V19");
        verify_v20_pine_source().expect("frozen V20 Pine source must match V20");
        verify_current_pine_source()
            .expect("current Research Pine must match explicit V20 research version");
    }

    #[test]
    fn v3_inherits_v2_additions_without_changing_legacy_current_pine() {
        assert!(ParityRuleVersion::Current3cbbc9d8.includes_v2_additions());
        assert!(!ParityRuleVersion::Current3cbbc9d8.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV3.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV3.includes_v3_guards());
        assert!(!ParityRuleVersion::CandidateV3.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV4.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV4.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV4.includes_v4_guards());
        assert!(!ParityRuleVersion::CandidateV4.includes_v5_guards());
        assert!(ParityRuleVersion::CandidateV5.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV5.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV5.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV5.includes_v5_guards());
        assert!(!ParityRuleVersion::CandidateV5.includes_v6_guards());
        assert!(ParityRuleVersion::CandidateV6.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV6.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV6.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV6.includes_v5_guards());
        assert!(ParityRuleVersion::CandidateV6.includes_v6_guards());
        assert!(!ParityRuleVersion::CandidateV6.includes_v7_guards());
        assert!(ParityRuleVersion::CandidateV7.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV7.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV7.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV7.includes_v5_guards());
        assert!(!ParityRuleVersion::CandidateV7.includes_v6_guards());
        assert!(ParityRuleVersion::CandidateV7.includes_v7_guards());
        assert!(!ParityRuleVersion::CandidateV7.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV8.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV8.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV8.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV8.includes_v5_guards());
        assert!(!ParityRuleVersion::CandidateV8.includes_v6_guards());
        assert!(!ParityRuleVersion::CandidateV8.includes_v7_guards());
        assert!(ParityRuleVersion::CandidateV8.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV9.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV9.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV9.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV9.includes_v5_guards());
        assert!(!ParityRuleVersion::CandidateV9.includes_v6_guards());
        assert!(!ParityRuleVersion::CandidateV9.includes_v7_guards());
        assert!(ParityRuleVersion::CandidateV9.includes_v8_guards());
        assert!(!ParityRuleVersion::CandidateV9.includes_v10_guards());
        assert!(ParityRuleVersion::CandidateV10.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV10.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV10.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV10.includes_v5_guards());
        assert!(!ParityRuleVersion::CandidateV10.includes_v6_guards());
        assert!(!ParityRuleVersion::CandidateV10.includes_v7_guards());
        assert!(ParityRuleVersion::CandidateV10.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV10.includes_v10_guards());
        assert!(!ParityRuleVersion::CandidateV10.includes_v11_guards());
        assert!(ParityRuleVersion::CandidateV11.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV11.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV11.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV11.includes_v5_guards());
        assert!(!ParityRuleVersion::CandidateV11.includes_v6_guards());
        assert!(!ParityRuleVersion::CandidateV11.includes_v7_guards());
        assert!(ParityRuleVersion::CandidateV11.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV11.includes_v10_guards());
        assert!(ParityRuleVersion::CandidateV11.includes_v11_guards());
        assert!(ParityRuleVersion::CandidateV12.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV12.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV12.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV12.includes_v5_guards());
        assert!(ParityRuleVersion::CandidateV12.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV12.includes_v10_guards());
        assert!(!ParityRuleVersion::CandidateV12.includes_v11_guards());
        assert!(ParityRuleVersion::CandidateV12.includes_v12_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV13.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v5_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v10_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v11_guards());
        assert!(!ParityRuleVersion::CandidateV13.includes_v12_guards());
        assert!(ParityRuleVersion::CandidateV13.includes_v13_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV14.includes_v3_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v4_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v5_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v8_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v10_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v11_guards());
        assert!(!ParityRuleVersion::CandidateV14.includes_v12_guards());
        assert!(!ParityRuleVersion::CandidateV14.includes_v13_guards());
        assert!(ParityRuleVersion::CandidateV14.includes_v14_guards());
        assert!(!ParityRuleVersion::CandidateV15.includes_v2_additions());
        assert!(!ParityRuleVersion::CandidateV15.includes_v14_guards());
        assert!(ParityRuleVersion::CandidateV15.includes_v15_range_squeeze());
        assert!(!ParityRuleVersion::CandidateV15.uses_right_side_trigger());
        assert!(!ParityRuleVersion::CandidateV16.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV16.includes_v15_range_squeeze());
        assert!(ParityRuleVersion::CandidateV16.uses_right_side_trigger());
        assert!(ParityRuleVersion::CandidateV16.uses_v16_economic_gates());
        assert!(!ParityRuleVersion::CandidateV17.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV17.includes_v15_range_squeeze());
        assert!(ParityRuleVersion::CandidateV17.uses_right_side_trigger());
        assert!(!ParityRuleVersion::CandidateV17.uses_v16_economic_gates());
        assert!(ParityRuleVersion::CandidateV18.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV18.includes_v11_guards());
        assert!(ParityRuleVersion::CandidateV18.includes_v15_range_squeeze());
        assert!(ParityRuleVersion::CandidateV18.uses_right_side_trigger());
        assert!(!ParityRuleVersion::CandidateV18.uses_v16_economic_gates());
        assert!(ParityRuleVersion::CandidateV18.is_v18_composite());
        assert!(ParityRuleVersion::CandidateV19.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV19.includes_v11_guards());
        assert!(ParityRuleVersion::CandidateV19.includes_v15_range_squeeze());
        assert!(ParityRuleVersion::CandidateV19.uses_right_side_trigger());
        assert!(ParityRuleVersion::CandidateV19.is_v19_composite());
        assert!(ParityRuleVersion::CandidateV19.rejects_false_breakout_short_on_long_lower_wick());
        assert!(!ParityRuleVersion::CandidateV19.enables_upthrust_failed_acceptance());
        assert!(ParityRuleVersion::CandidateV20.includes_v2_additions());
        assert!(ParityRuleVersion::CandidateV20.includes_v11_guards());
        assert!(ParityRuleVersion::CandidateV20.includes_v15_range_squeeze());
        assert!(ParityRuleVersion::CandidateV20.uses_right_side_trigger());
        assert!(ParityRuleVersion::CandidateV20.is_v19_composite());
        assert!(ParityRuleVersion::CandidateV20.rejects_false_breakout_short_on_long_lower_wick());
        assert!(ParityRuleVersion::CandidateV20.enables_upthrust_failed_acceptance());

        let legacy = ReplayConfig::current_pine("TEST", 0.1, 1, 2);
        let candidate = ReplayConfig::current_pine_v3("TEST", 0.1, 1, 2);
        let candidate_v4 = ReplayConfig::current_pine_v4("TEST", 0.1, 1, 2);
        let candidate_v5 = ReplayConfig::current_pine_v5("TEST", 0.1, 1, 2);
        let candidate_v6 = ReplayConfig::current_pine_v6("TEST", 0.1, 1, 2);
        let candidate_v7 = ReplayConfig::current_pine_v7("TEST", 0.1, 1, 2);
        let candidate_v8 = ReplayConfig::current_pine_v8("TEST", 0.1, 1, 2);
        let candidate_v9 = ReplayConfig::current_pine_v9("TEST", 0.1, 1, 2);
        let candidate_v10 = ReplayConfig::current_pine_v10("TEST", 0.1, 1, 2);
        let candidate_v11 = ReplayConfig::current_pine_v11("TEST", 0.1, 1, 2);
        let candidate_v12 = ReplayConfig::current_pine_v12("TEST", 0.1, 1, 2);
        let candidate_v13 = ReplayConfig::current_pine_v13("TEST", 0.1, 1, 2);
        let candidate_v14 = ReplayConfig::current_pine_v14("TEST", 0.1, 1, 2);
        let candidate_v15 = ReplayConfig::current_pine_v15("TEST", 0.1, 1, 2);
        let candidate_v16 = ReplayConfig::current_pine_v16("TEST", 0.1, 1, 2);
        let candidate_v17 = ReplayConfig::current_pine_v17("TEST", 0.1, 1, 2);
        let candidate_v18 = ReplayConfig::current_pine_v18("TEST", 0.1, 1, 2);
        let candidate_v19 = ReplayConfig::current_pine_v19("TEST", 0.1, 1, 2);
        let candidate_v20 = ReplayConfig::current_pine_v20("TEST", 0.1, 1, 2);
        assert_eq!(legacy.rule_version, ParityRuleVersion::Current3cbbc9d8);
        assert_eq!(candidate.rule_version, ParityRuleVersion::CandidateV3);
        assert_eq!(candidate_v4.rule_version, ParityRuleVersion::CandidateV4);
        assert_eq!(candidate_v5.rule_version, ParityRuleVersion::CandidateV5);
        assert_eq!(candidate_v6.rule_version, ParityRuleVersion::CandidateV6);
        assert_eq!(candidate_v7.rule_version, ParityRuleVersion::CandidateV7);
        assert_eq!(candidate_v8.rule_version, ParityRuleVersion::CandidateV8);
        assert_eq!(candidate_v9.rule_version, ParityRuleVersion::CandidateV9);
        assert_eq!(candidate_v10.rule_version, ParityRuleVersion::CandidateV10);
        assert_eq!(candidate_v11.rule_version, ParityRuleVersion::CandidateV11);
        assert_eq!(candidate_v12.rule_version, ParityRuleVersion::CandidateV12);
        assert_eq!(candidate_v13.rule_version, ParityRuleVersion::CandidateV13);
        assert_eq!(candidate_v14.rule_version, ParityRuleVersion::CandidateV14);
        assert_eq!(candidate_v15.rule_version, ParityRuleVersion::CandidateV15);
        assert_eq!(candidate_v16.rule_version, ParityRuleVersion::CandidateV16);
        assert_eq!(candidate_v17.rule_version, ParityRuleVersion::CandidateV17);
        assert_eq!(candidate_v18.rule_version, ParityRuleVersion::CandidateV18);
        assert_eq!(candidate_v19.rule_version, ParityRuleVersion::CandidateV19);
        assert_eq!(candidate_v20.rule_version, ParityRuleVersion::CandidateV20);
    }

    #[test]
    fn flat_confirmed_candle_is_kept_for_indicator_continuity() {
        assert!(Candle {
            timestamp_ms: 0,
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: 10.0,
        }
        .is_valid());
    }
}
