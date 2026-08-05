//! V2：EMA144/576 历史资格锁存后的可重复动态 EMA144 首次回踩。
//!
//! active 方向只有完整镜像转换才能切换；每次回踩必须先由已完成 K 重新远离 EMA144。

mod independent_active_v5;
mod report;
pub use report::*;

use super::super::{config_from_env_and_args, load_backtest_data, BacktestDataSet, MS_15M};
use super::{
    breakout_at, complete_window_bounds, ema_close_series, excluded_symbol, frozen_l1_args,
    hash_symbol_window, pattern_bars, update_target_input_coverage, Direction, ExcludedSymbol,
    L1Coverage, L1Decision, PatternBar, ReadyBar, TargetAudit, EMA_SLOW_PERIOD, EVALUATION_END_MS,
    EVALUATION_START_MS, REQUIRED_PRE_EVALUATION_BARS, TARGETS,
};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// V2 新策略能力身份，V1 失败证据保持不变。
pub const CANDIDATE_KEY: &str = "market_momentum_ema144_576_persistent_dynamic_retest_15m_v2";
/// V2 L1 冻结规则版本。
pub const RULE_VERSION: &str = "l1_age144_transition_latch_reexpand075atr_first_touch030atr_v2";
/// V3 让多空资格各自在 EMA576 周期内保持可复用。
pub const V3_CANDIDATE_KEY: &str =
    "market_momentum_ema144_576_recent_qualification_dynamic_retest_15m_v3";
/// V3 只改变资格记忆策略，回踩与方向转换阈值保持 V2 不变。
pub const V3_RULE_VERSION: &str =
    "l1_age144_recent576_dual_transition_reexpand075atr_first_touch030atr_v3";
/// V4 从持续资格状态的最后确认 K 开始计算 576 根有效期。
pub const V4_CANDIDATE_KEY: &str =
    "market_momentum_ema144_576_sustained_qualification_dynamic_retest_15m_v4";
/// V4 只修正资格时间戳刷新口径。
pub const V4_RULE_VERSION: &str =
    "l1_age144_refresh_while_sustained_recent576_transition_reexpand075atr_first_touch030atr_v4";
/// V5 保存多空各自的 active latch，由当前价格所在慢线一侧选择回踩方向。
pub const V5_CANDIDATE_KEY: &str = "market_momentum_ema144_576_dual_active_dynamic_retest_15m_v5";
/// V5 只改变 active direction 的所有权与过期方式。
pub const V5_RULE_VERSION: &str =
    "l1_age144_refresh_recent576_dual_active_price_side_reexpand075atr_first_touch030atr_v5";
/// V6 永久保留已经成立的历史资格，并让已武装回踩订单跨越 EMA576 存活。
pub const V6_CANDIDATE_KEY: &str =
    "market_momentum_ema144_576_persistent_qualification_order_retest_15m_v6";
/// V6 只改变资格与回踩订单的生命周期，形态阈值保持不变。
pub const V6_RULE_VERSION: &str =
    "l1_age144_persistent_dual_transition_latest_arm_reexpand075atr_first_touch030atr_v6";
/// V8 永久保留历史资格，但让价格 transition 按 EMA576 双收盘穿越形成有限 episode。
pub const V8_CANDIDATE_KEY: &str = "market_momentum_ema144_576_finite_price_episode_retest_15m_v8";
/// V8 只改变 active transition 生命周期，不增加信号过滤条件。
pub const V8_RULE_VERSION: &str =
    "l1_persistent_qualification_finite_price_episode_latest_arm_reexpand075atr_first_touch030atr_v8";

const REQUIRED_QUALIFICATION_BARS: usize = 144;
const TRANSITION_WINDOW_BARS: usize = 24;
const TRANSITION_DEPARTURE_ATR: f64 = 0.75;
const RETEST_REEXPANSION_ATR: f64 = 0.75;
const RETEST_TOUCH_BUFFER_ATR: f64 = 0.30;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;
const QUALIFICATION_MEMORY_BARS: usize = EMA_SLOW_PERIOD;

#[derive(Debug, Clone, Copy)]
enum QualificationPolicy {
    LatestOnly,
    RecentDual {
        max_age_bars: usize,
        refresh_while_sustained: bool,
    },
}

#[derive(Debug, Clone, Copy)]
enum ActivePolicy {
    Exclusive,
    Independent {
        qualification_max_age_bars: Option<usize>,
        clear_arm_off_price_side: bool,
        finite_price_episode: bool,
    },
}

#[derive(Debug, Clone, Copy)]
struct ResearchVariant {
    schema_version: &'static str,
    candidate_key: &'static str,
    rule_version: &'static str,
    only_variable: &'static str,
    qualification_memory_policy: &'static str,
    transition_latch_policy: &'static str,
    runtime_boundary: &'static str,
    qualification_policy: QualificationPolicy,
    active_policy: ActivePolicy,
}

const V2_VARIANT: ResearchVariant = ResearchVariant {
    schema_version: "market_momentum_ema144_576_persistent_dynamic_retest_l1_v2",
    candidate_key: CANDIDATE_KEY,
    rule_version: RULE_VERSION,
    only_variable: "replace one-shot-per-EMA-regime eligibility with a persistent transition latch and one first EMA144 touch after each completed 0.75 ATR re-expansion",
    qualification_memory_policy: "only the most recently completed 144-bar EMA qualification direction can start a new price transition",
    transition_latch_policy: "a 144-bar EMA qualification becomes active only after a two-close EMA576 break plus 0.75 ATR departure; active direction persists until the full mirror qualification and price transition completes",
    runtime_boundary: "research-only V2; not registered in paper, readonly shadow, live worker, compose, or production presets",
    qualification_policy: QualificationPolicy::LatestOnly,
    active_policy: ActivePolicy::Exclusive,
};

const V3_VARIANT: ResearchVariant = ResearchVariant {
    schema_version: "market_momentum_ema144_576_recent_qualification_dynamic_retest_l1_v3",
    candidate_key: V3_CANDIDATE_KEY,
    rule_version: V3_RULE_VERSION,
    only_variable: "change only qualification memory from latest-direction-only to independent long and short 144-bar qualifications valid for the following 576 completed 15m candles",
    qualification_memory_policy: "long and short qualifications are stored independently and each expires 576 completed 15m candles after qualification",
    transition_latch_policy: "an unexpired same-direction qualification can start a two-close EMA576 transition plus 0.75 ATR departure; active changes only when another complete qualified price transition activates",
    runtime_boundary: "research-only V3; not registered in paper, readonly shadow, live worker, compose, or production presets",
    qualification_policy: QualificationPolicy::RecentDual {
        max_age_bars: QUALIFICATION_MEMORY_BARS,
        refresh_while_sustained: false,
    },
    active_policy: ActivePolicy::Exclusive,
};

const V4_VARIANT: ResearchVariant = ResearchVariant {
    schema_version: "market_momentum_ema144_576_sustained_qualification_dynamic_retest_l1_v4",
    candidate_key: V4_CANDIDATE_KEY,
    rule_version: V4_RULE_VERSION,
    only_variable: "change only the V3 qualification timestamp so an already-qualified EMA side refreshes on every additional completed candle while the same strict EMA144-vs-EMA576 relation remains true",
    qualification_memory_policy: "long and short qualifications are independent; after a side reaches 144 bars its timestamp refreshes while sustained, then expires 576 bars after that sustained relation ends",
    transition_latch_policy: "an unexpired same-direction qualification can start a two-close EMA576 transition plus 0.75 ATR departure; active changes only when another complete qualified price transition activates",
    runtime_boundary: "research-only V4; not registered in paper, readonly shadow, live worker, compose, or production presets",
    qualification_policy: QualificationPolicy::RecentDual {
        max_age_bars: QUALIFICATION_MEMORY_BARS,
        refresh_while_sustained: true,
    },
    active_policy: ActivePolicy::Exclusive,
};

const V5_VARIANT: ResearchVariant = ResearchVariant {
    schema_version: "market_momentum_ema144_576_dual_active_dynamic_retest_l1_v5",
    candidate_key: V5_CANDIDATE_KEY,
    rule_version: V5_RULE_VERSION,
    only_variable: "change only active ownership from one globally exclusive direction to independent long and short transition latches; the completed close side of EMA576 selects which direction may arm",
    qualification_memory_policy: "long and short qualifications are independent; after a side reaches 144 bars its timestamp refreshes while sustained, then expires 576 bars after that sustained relation ends",
    transition_latch_policy: "each direction keeps its own qualified two-close EMA576 transition latch until that direction qualification expires; the opposite transition does not delete it",
    runtime_boundary: "research-only V5; not registered in paper, readonly shadow, live worker, compose, or production presets",
    qualification_policy: QualificationPolicy::RecentDual {
        max_age_bars: QUALIFICATION_MEMORY_BARS,
        refresh_while_sustained: true,
    },
    active_policy: ActivePolicy::Independent {
        qualification_max_age_bars: Some(QUALIFICATION_MEMORY_BARS),
        clear_arm_off_price_side: true,
        finite_price_episode: false,
    },
};

const V6_VARIANT: ResearchVariant = ResearchVariant {
    schema_version: "market_momentum_ema144_576_persistent_qualification_order_retest_l1_v6",
    candidate_key: V6_CANDIDATE_KEY,
    rule_version: V6_RULE_VERSION,
    only_variable: "persist an established 144-bar qualification and its effective transition without a 576-bar expiry, and keep the latest armed EMA144 retest order alive across an interim EMA576 recross until first touch",
    qualification_memory_policy: "long and short qualifications are stored independently and persist after 144 completed qualifying candles; only an input gap or unavailable required indicator resets state",
    transition_latch_policy: "long and short effective transitions persist independently; only the latest direction to complete a same-side 0.75 ATR re-expansion owns the pending first-touch order, which an interim EMA576 recross does not cancel",
    runtime_boundary: "research-only V6; not registered in paper, readonly shadow, live worker, compose, or production presets",
    qualification_policy: QualificationPolicy::RecentDual {
        max_age_bars: QUALIFICATION_MEMORY_BARS,
        refresh_while_sustained: true,
    },
    active_policy: ActivePolicy::Independent {
        qualification_max_age_bars: None,
        clear_arm_off_price_side: false,
        finite_price_episode: false,
    },
};

const V8_VARIANT: ResearchVariant = ResearchVariant {
    schema_version: "market_momentum_ema144_576_finite_price_episode_retest_l1_v8",
    candidate_key: V8_CANDIDATE_KEY,
    rule_version: V8_RULE_VERSION,
    only_variable: "keep the established 144-bar qualification persistent while ending only the active price-transition episode after an opposite two-close EMA576 break; a previously armed order survives until touch",
    qualification_memory_policy: "long and short 144-bar qualifications remain independent and persistent; only an input gap or unavailable required indicator resets them",
    transition_latch_policy: "a qualified effective breakout opens a finite price episode; an opposite two-close EMA576 break ends new arming, while an already resting EMA144 retest order remains valid until touch or a newer transition context replaces it",
    runtime_boundary: "research-only V8; not registered in paper, readonly shadow, live worker, compose, or production presets",
    qualification_policy: QualificationPolicy::RecentDual {
        max_age_bars: QUALIFICATION_MEMORY_BARS,
        refresh_while_sustained: true,
    },
    active_policy: ActivePolicy::Independent {
        qualification_max_age_bars: None,
        clear_arm_off_price_side: false,
        finite_price_episode: true,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Qualification {
    Long,
    Short,
}

impl Qualification {
    fn direction(self) -> Direction {
        match self {
            Self::Long => Direction::Long,
            Self::Short => Direction::Short,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct QualifiedState {
    direction: Qualification,
    qualified_idx: usize,
    qualified_ts: i64,
}

#[derive(Debug, Clone, Copy)]
struct PendingTransition {
    direction: Direction,
    breakout_idx: usize,
    breakout_ts: i64,
    qualified_ts: i64,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTransition {
    direction: Direction,
    qualified_ts: i64,
    breakout_ts: i64,
    activated_idx: usize,
    activated_ts: i64,
}

#[derive(Debug, Clone, Copy)]
struct RetestArm {
    direction: Direction,
    armed_idx: usize,
    armed_ts: i64,
}

#[derive(Debug, Clone, Copy)]
struct CandidateCore {
    direction: Direction,
    signal_idx: usize,
    signal_bar: ReadyBar,
    active: ActiveTransition,
    arm: RetestArm,
    anchor: ReadyBar,
}

#[derive(Debug, Default)]
struct StepResult {
    qualification_changed: bool,
    transition_breakout: bool,
    active_transition: bool,
    retest_armed: bool,
    retest_touched: bool,
    candidate: Option<CandidateCore>,
}

#[derive(Debug)]
struct PersistentTransitionMachine {
    qualification_policy: QualificationPolicy,
    long_age: usize,
    short_age: usize,
    qualified: Option<QualifiedState>,
    long_qualified: Option<QualifiedState>,
    short_qualified: Option<QualifiedState>,
    pending: Option<PendingTransition>,
    active: Option<ActiveTransition>,
    retest_arm: Option<RetestArm>,
}

impl PersistentTransitionMachine {
    fn new_with_policy(qualification_policy: QualificationPolicy) -> Self {
        Self {
            qualification_policy,
            long_age: 0,
            short_age: 0,
            qualified: None,
            long_qualified: None,
            short_qualified: None,
            pending: None,
            active: None,
            retest_arm: None,
        }
    }

    /// 状态顺序固定为：处理已预置回踩 -> 更新均线资格 -> 完成方向转换 -> 收盘重新武装。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.long_age = 0;
            self.short_age = 0;
            self.pending = None;
            self.retest_arm = None;
            return StepResult::default();
        };
        let mut result = StepResult::default();
        let touched = self.try_retest_touch(bars, idx, bar, &mut result);
        self.update_qualification(idx, bar, &mut result);
        self.update_transition(bars, idx, bar, &mut result);
        if !touched {
            self.try_arm_retest(idx, bar, &mut result);
        }
        result
    }

    fn try_retest_touch(
        &mut self,
        bars: &[PatternBar],
        idx: usize,
        bar: ReadyBar,
        result: &mut StepResult,
    ) -> bool {
        let (Some(active), Some(arm), Some(previous_idx)) =
            (self.active, self.retest_arm, idx.checked_sub(1))
        else {
            return false;
        };
        if active.direction != arm.direction || idx <= arm.armed_idx {
            return false;
        }
        let Some(anchor) = bars[previous_idx].ready() else {
            self.retest_arm = None;
            return false;
        };
        let touched = match active.direction {
            Direction::Long => bar.low <= anchor.ema144 + RETEST_TOUCH_BUFFER_ATR * anchor.atr14,
            Direction::Short => bar.high >= anchor.ema144 - RETEST_TOUCH_BUFFER_ATR * anchor.atr14,
        };
        if !touched {
            return false;
        }
        result.retest_touched = true;
        result.candidate = Some(CandidateCore {
            direction: active.direction,
            signal_idx: idx,
            signal_bar: bar,
            active,
            arm,
            anchor,
        });
        self.retest_arm = None;
        true
    }

    fn update_qualification(&mut self, idx: usize, bar: ReadyBar, result: &mut StepResult) {
        if bar.ema144 < bar.ema576 {
            self.long_age = self.long_age.saturating_add(1);
            self.short_age = 0;
            if self.long_age == REQUIRED_QUALIFICATION_BARS {
                self.set_qualification(Qualification::Long, idx, bar.ts, result);
            } else if self.long_age > REQUIRED_QUALIFICATION_BARS {
                self.refresh_sustained_qualification(Qualification::Long, idx, bar.ts);
            }
        } else if bar.ema144 > bar.ema576 {
            self.short_age = self.short_age.saturating_add(1);
            self.long_age = 0;
            if self.short_age == REQUIRED_QUALIFICATION_BARS {
                self.set_qualification(Qualification::Short, idx, bar.ts, result);
            } else if self.short_age > REQUIRED_QUALIFICATION_BARS {
                self.refresh_sustained_qualification(Qualification::Short, idx, bar.ts);
            }
        } else {
            self.long_age = 0;
            self.short_age = 0;
        }
    }

    fn set_qualification(
        &mut self,
        direction: Qualification,
        idx: usize,
        ts: i64,
        result: &mut StepResult,
    ) {
        let state = QualifiedState {
            direction,
            qualified_idx: idx,
            qualified_ts: ts,
        };
        match direction {
            Qualification::Long => self.long_qualified = Some(state),
            Qualification::Short => self.short_qualified = Some(state),
        }
        if matches!(self.qualification_policy, QualificationPolicy::LatestOnly) {
            if self
                .qualified
                .is_some_and(|qualified| qualified.direction == direction)
            {
                return;
            }
            self.qualified = Some(state);
            self.pending = None;
        }
        result.qualification_changed = true;
    }

    fn refresh_sustained_qualification(&mut self, direction: Qualification, idx: usize, ts: i64) {
        let QualificationPolicy::RecentDual {
            refresh_while_sustained: true,
            ..
        } = self.qualification_policy
        else {
            return;
        };
        let state = QualifiedState {
            direction,
            qualified_idx: idx,
            qualified_ts: ts,
        };
        match direction {
            Qualification::Long => self.long_qualified = Some(state),
            Qualification::Short => self.short_qualified = Some(state),
        }
    }

    fn qualification_for_direction(
        &self,
        direction: Direction,
        idx: usize,
    ) -> Option<QualifiedState> {
        match self.qualification_policy {
            QualificationPolicy::LatestOnly => self
                .qualified
                .filter(|qualified| qualified.direction.direction() == direction),
            QualificationPolicy::RecentDual { max_age_bars, .. } => {
                let qualified = match direction {
                    Direction::Long => self.long_qualified,
                    Direction::Short => self.short_qualified,
                }?;
                (idx.saturating_sub(qualified.qualified_idx) <= max_age_bars).then_some(qualified)
            }
        }
    }

    fn update_transition(
        &mut self,
        bars: &[PatternBar],
        idx: usize,
        bar: ReadyBar,
        result: &mut StepResult,
    ) {
        if let Some(pending) = self.pending {
            let still_qualified = self
                .qualification_for_direction(pending.direction, idx)
                .is_some();
            let elapsed = idx.saturating_sub(pending.breakout_idx);
            if !still_qualified || elapsed >= TRANSITION_WINDOW_BARS {
                self.pending = None;
            } else if departed_from_slow(pending.direction, bar) {
                self.activate(pending, idx, bar.ts, result);
            }
        }

        if self.pending.is_some() {
            return;
        }
        for direction in [Direction::Long, Direction::Short] {
            let Some(qualified) = self.qualification_for_direction(direction, idx) else {
                continue;
            };
            if self
                .active
                .is_some_and(|active| active.direction == direction)
                || !breakout_at(bars, idx, direction)
            {
                continue;
            }
            let pending = PendingTransition {
                direction,
                breakout_idx: idx,
                breakout_ts: bar.ts,
                qualified_ts: qualified.qualified_ts,
            };
            result.transition_breakout = true;
            self.pending = Some(pending);
            if departed_from_slow(direction, bar) {
                self.activate(pending, idx, bar.ts, result);
            }
            break;
        }
    }

    fn activate(
        &mut self,
        pending: PendingTransition,
        idx: usize,
        ts: i64,
        result: &mut StepResult,
    ) {
        self.active = Some(ActiveTransition {
            direction: pending.direction,
            qualified_ts: pending.qualified_ts,
            breakout_ts: pending.breakout_ts,
            activated_idx: idx,
            activated_ts: ts,
        });
        self.pending = None;
        self.retest_arm = None;
        result.active_transition = true;
    }

    fn try_arm_retest(&mut self, idx: usize, bar: ReadyBar, result: &mut StepResult) {
        let Some(active) = self.active else {
            return;
        };
        if self.retest_arm.is_some() || !reexpanded_from_fast(active.direction, bar) {
            return;
        }
        self.retest_arm = Some(RetestArm {
            direction: active.direction,
            armed_idx: idx,
            armed_ts: bar.ts,
        });
        result.retest_armed = true;
    }
}

/// 连接本机 quant_core，生成 V2 L1 无标签机器报告。
pub async fn run_l1_scan(output: &Path) -> Result<V2Report> {
    run_variant_l1_scan(output, V2_VARIANT).await
}

/// 使用相同数据和回踩合同运行 V3 的 576 根双向资格记忆版本。
pub async fn run_v3_l1_scan(output: &Path) -> Result<V2Report> {
    run_variant_l1_scan(output, V3_VARIANT).await
}

/// 运行 V4：持续同侧状态逐根刷新资格时间戳。
pub async fn run_v4_l1_scan(output: &Path) -> Result<V2Report> {
    run_variant_l1_scan(output, V4_VARIANT).await
}

/// 运行 V5：多空 transition latch 独立存活，由当前 EMA576 价格侧选择回踩方向。
pub async fn run_v5_l1_scan(output: &Path) -> Result<V2Report> {
    run_variant_l1_scan(output, V5_VARIANT).await
}

/// 运行 V6：历史资格永久锁存，最新武装的 EMA144 回踩订单直到首次触碰才消费。
pub async fn run_v6_l1_scan(output: &Path) -> Result<V2Report> {
    run_variant_l1_scan(output, V6_VARIANT).await
}

/// 运行 V8：永久资格与有限价格 episode 分离，已武装订单仍跨 EMA576 存活。
pub async fn run_v8_l1_scan(output: &Path) -> Result<V2Report> {
    run_variant_l1_scan(output, V8_VARIANT).await
}

async fn run_variant_l1_scan(output: &Path, variant: ResearchVariant) -> Result<V2Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for persistent EMA retest L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_report(&data, variant)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_report(data: &BacktestDataSet, variant: ResearchVariant) -> Result<V2Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * MS_15M)
        .context("V2 warmup start overflow")?;
    let expected_window_candles =
        super::inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols: Vec<ExcludedSymbol> = Vec::new();
    let mut candidates = Vec::new();
    let mut stages = V2StageCounts::default();
    let mut eligible_symbols = BTreeSet::new();
    let mut target_inputs = super::target_input_template();
    let mut hasher = Sha256::new();
    let mut pairs = data.pairs.iter().collect::<Vec<_>>();
    pairs.sort_by(|left, right| left.symbol.cmp(&right.symbol));

    for pair in pairs {
        let candles = data
            .candles_15m_computed
            .get(&pair.symbol)
            .with_context(|| format!("missing computed candles for {}", pair.symbol))?;
        let Some((start_idx, end_idx)) =
            complete_window_bounds(candles, warmup_start_ms, EVALUATION_END_MS)
        else {
            excluded_symbols.push(excluded_symbol(
                &pair.symbol,
                candles,
                warmup_start_ms,
                EVALUATION_END_MS,
                expected_window_candles,
            ));
            continue;
        };
        let ema576 = ema_close_series(candles, EMA_SLOW_PERIOD);
        let bars = pattern_bars(candles, &ema576);
        let evaluation_start_idx = start_idx
            .checked_add(REQUIRED_PRE_EVALUATION_BARS)
            .context("V2 evaluation start index overflow")?;
        if candles
            .get(evaluation_start_idx)
            .is_none_or(|candle| candle.candle.ts != EVALUATION_START_MS)
        {
            bail!("eligible window lost evaluation start for {}", pair.symbol);
        }
        if bars[evaluation_start_idx..=end_idx]
            .iter()
            .any(|bar| bar.ready().is_none())
        {
            excluded_symbols.push(ExcludedSymbol {
                symbol: pair.symbol.clone(),
                expected_candles: expected_window_candles,
                loaded_candles: expected_window_candles,
                missing_candles: 0,
                reason: "ema144_ema576_or_atr14_not_ready_in_required_window",
            });
            continue;
        }
        eligible_symbols.insert(pair.symbol.clone());
        hash_symbol_window(&mut hasher, &pair.symbol, &bars[start_idx..=end_idx]);
        update_target_input_coverage(&pair.symbol, &bars, &mut target_inputs);
        scan_symbol(
            &pair.symbol,
            &bars,
            start_idx,
            end_idx,
            variant,
            &mut candidates,
            &mut stages,
        )?;
    }
    excluded_symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.signal_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    let target_audits = audit_targets(&candidates);
    let summary = summarize(&candidates, stages);
    let decision = decide(&summary, &target_audits, &target_inputs, variant);

    Ok(V2Report {
        schema_version: variant.schema_version,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V2Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: variant.candidate_key,
            rule_version: variant.rule_version,
            only_variable: variant.only_variable,
            qualification_memory_policy: variant.qualification_memory_policy,
            transition_latch_policy: variant.transition_latch_policy,
            causal_order_anchor_policy: "the touch candle uses the previous completed candle EMA144 and ATR14, so a resting order could be prepared before the touch candle begins",
            label_boundary: "no fill, future candle, MFE, MAE, exit, R, win, loss, or PnL field is read",
            runtime_boundary: variant.runtime_boundary,
        },
        coverage: L1Coverage {
            expected_symbol_count: 60,
            returned_symbol_count: data.pairs.len(),
            eligible_symbol_count: eligible_symbols.len(),
            excluded_symbols,
            evaluation_start_ms: EVALUATION_START_MS,
            evaluation_end_ms: EVALUATION_END_MS,
            required_pre_evaluation_bars: REQUIRED_PRE_EVALUATION_BARS,
            target_inputs,
            dataset_fingerprint_sha256: hex::encode(hasher.finalize()),
            universe_limitation: "current-live Top60 is a survivorship-biased L1 diagnostic; missing local members are skipped and no candles are backfilled",
        },
        summary,
        target_audits,
        decision,
        candidates,
    })
}

/// 为后续等级在同一行情快照上重建 V6 L1 身份，不读取任何成交后字段。
pub(super) fn build_v6_l1_report(data: &BacktestDataSet) -> Result<V2Report> {
    build_report(data, V6_VARIANT)
}

fn scan_symbol(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    variant: ResearchVariant,
    candidates: &mut Vec<V2Candidate>,
    stages: &mut V2StageCounts,
) -> Result<()> {
    if let ActivePolicy::Independent {
        qualification_max_age_bars,
        clear_arm_off_price_side,
        finite_price_episode,
    } = variant.active_policy
    {
        return independent_active_v5::scan_symbol(
            symbol,
            bars,
            start_idx,
            end_idx,
            qualification_max_age_bars,
            clear_arm_off_price_side,
            finite_price_episode,
            candidates,
            stages,
        );
    }
    let mut machine = PersistentTransitionMachine::new_with_policy(variant.qualification_policy);
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualification_changes += usize::from(step.qualification_changed);
        stages.transition_breakouts += usize::from(step.transition_breakout);
        stages.active_transitions += usize::from(step.active_transition);
        stages.retest_arms += usize::from(step.retest_armed);
        stages.retest_touches += usize::from(step.retest_touched);
        if let Some(core) = step.candidate {
            candidates.push(candidate_from_core(symbol, core)?);
        }
    }
    Ok(())
}

fn candidate_from_core(symbol: &str, core: CandidateCore) -> Result<V2Candidate> {
    let signal_month_utc = Utc
        .timestamp_millis_opt(core.signal_bar.ts)
        .single()
        .context("invalid V2 candidate timestamp")?
        .format("%Y-%m")
        .to_string();
    let (touch_zone_boundary, touch_extreme, extreme_to_anchor, close_to_current, close_holds) =
        match core.direction {
            Direction::Long => (
                core.anchor.ema144 + RETEST_TOUCH_BUFFER_ATR * core.anchor.atr14,
                core.signal_bar.low,
                (core.signal_bar.low - core.anchor.ema144) / core.anchor.atr14,
                (core.signal_bar.close - core.signal_bar.ema144) / core.signal_bar.atr14,
                core.signal_bar.close >= core.signal_bar.ema144,
            ),
            Direction::Short => (
                core.anchor.ema144 - RETEST_TOUCH_BUFFER_ATR * core.anchor.atr14,
                core.signal_bar.high,
                (core.anchor.ema144 - core.signal_bar.high) / core.anchor.atr14,
                (core.signal_bar.ema144 - core.signal_bar.close) / core.signal_bar.atr14,
                core.signal_bar.close <= core.signal_bar.ema144,
            ),
        };
    Ok(V2Candidate {
        symbol: symbol.to_owned(),
        direction: core.direction.label(),
        signal_ts_ms: core.signal_bar.ts,
        signal_month_utc,
        qualified_ts_ms: core.active.qualified_ts,
        breakout_ts_ms: core.active.breakout_ts,
        active_since_ts_ms: core.active.activated_ts,
        reexpanded_ts_ms: core.arm.armed_ts,
        bars_since_activation: core.signal_idx.saturating_sub(core.active.activated_idx),
        bars_since_reexpansion: core.signal_idx.saturating_sub(core.arm.armed_idx),
        anchor_ema144: core.anchor.ema144,
        anchor_atr14: core.anchor.atr14,
        touch_zone_boundary,
        touch_extreme,
        touch_extreme_to_anchor_atr: extreme_to_anchor,
        close_to_current_ema144_atr: close_to_current,
        close_holds_current_ema144: close_holds,
        cross_phase: core.direction.cross_phase(core.signal_bar),
        current_ema144: core.signal_bar.ema144,
        current_ema576: core.signal_bar.ema576,
        current_atr14: core.signal_bar.atr14,
    })
}

fn departed_from_slow(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => bar.close - bar.ema576 >= TRANSITION_DEPARTURE_ATR * bar.atr14,
        Direction::Short => bar.ema576 - bar.close >= TRANSITION_DEPARTURE_ATR * bar.atr14,
    }
}

fn reexpanded_from_fast(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => bar.close - bar.ema144 >= RETEST_REEXPANSION_ATR * bar.atr14,
        Direction::Short => bar.ema144 - bar.close >= RETEST_REEXPANSION_ATR * bar.atr14,
    }
}

fn audit_targets(candidates: &[V2Candidate]) -> Vec<TargetAudit> {
    TARGETS
        .iter()
        .map(|target| {
            let matched_signal_timestamps_ms = candidates
                .iter()
                .filter(|candidate| {
                    candidate.symbol == target.symbol
                        && candidate.direction == target.direction.label()
                        && (target.start_ms..=target.end_ms).contains(&candidate.signal_ts_ms)
                })
                .map(|candidate| candidate.signal_ts_ms)
                .collect::<Vec<_>>();
            TargetAudit {
                name: target.name,
                symbol: target.symbol,
                direction: target.direction.label(),
                start_ms: target.start_ms,
                end_ms: target.end_ms,
                matched: !matched_signal_timestamps_ms.is_empty(),
                matched_signal_timestamps_ms,
            }
        })
        .collect()
}

fn summarize(candidates: &[V2Candidate], stages: V2StageCounts) -> V2Summary {
    let mut by_direction = BTreeMap::new();
    let mut by_cross_phase = BTreeMap::new();
    let mut by_close_hold = BTreeMap::new();
    let mut by_symbol = BTreeMap::new();
    let mut by_month_utc = BTreeMap::new();
    for candidate in candidates {
        *by_direction.entry(candidate.direction).or_default() += 1;
        *by_cross_phase.entry(candidate.cross_phase).or_default() += 1;
        *by_close_hold
            .entry(if candidate.close_holds_current_ema144 {
                "close_holds"
            } else {
                "close_lost"
            })
            .or_default() += 1;
        *by_symbol.entry(candidate.symbol.clone()).or_default() += 1;
        *by_month_utc
            .entry(candidate.signal_month_utc.clone())
            .or_default() += 1;
    }
    V2Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_close_hold,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_market_event_count(candidates),
        stages,
    }
}

fn effective_market_event_count(candidates: &[V2Candidate]) -> usize {
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for candidate in candidates {
        let starts_new = last_by_direction
            .get(candidate.direction)
            .is_none_or(|previous| candidate.signal_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(candidate.direction, candidate.signal_ts_ms);
    }
    count
}

fn decide(
    summary: &V2Summary,
    audits: &[TargetAudit],
    target_inputs: &[super::TargetInputCoverage],
    variant: ResearchVariant,
) -> L1Decision {
    let long_count = summary
        .by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let targets_match = audits.iter().all(|audit| audit.matched);
    let targets_ready = target_inputs.iter().all(|coverage| coverage.ready);
    let mut gates = BTreeMap::new();
    gates.insert("all_three_user_targets_match", targets_match);
    gates.insert("candidates_at_least_10", summary.candidate_count >= 10);
    gates.insert(
        "effective_events_at_least_5",
        summary.effective_market_events >= 5,
    );
    gates.insert("symbols_at_least_4", summary.by_symbol.len() >= 4);
    gates.insert("utc_months_at_least_3", summary.by_month_utc.len() >= 3);
    gates.insert(
        "both_directions_at_least_2",
        long_count >= 2 && short_count >= 2,
    );
    gates.insert("btc_and_nmr_inputs_ready", targets_ready);
    if variant.candidate_key == V8_CANDIDATE_KEY {
        const V6_BASELINE_CANDIDATES: usize = 54_837;
        let reduction_pct = V6_BASELINE_CANDIDATES.saturating_sub(summary.candidate_count) as f64
            / V6_BASELINE_CANDIDATES as f64
            * 100.0;
        // 该门禁验证有限 episode 确实改变了生命周期，避免 3/3 目标掩盖近似无效的状态改动。
        gates.insert(
            "v8_candidate_reduction_between_30_and_85_pct",
            (30.0..=85.0).contains(&reduction_pct),
        );
    }
    let all_pass = gates.values().all(|passed| *passed);
    let (status, reason) = if !targets_match {
        (
            "rejected_definition_mismatch",
            "当前版本仍有用户目标图未命中；按预注册停止，不读取收益或继续调整阈值。",
        )
    } else if !all_pass {
        (
            "stop_coverage_gate_failed",
            "用户目标已匹配，但无标签覆盖或分散性至少一项失败；不进入 L2。",
        )
    } else {
        (
            "coverage_pass_ready_for_l2_prereg",
            "当前版本目标定义、覆盖和分散性通过；下一步只能先冻结 L2 成本回放合同。",
        )
    };
    L1Decision {
        status,
        gates,
        reason: reason.to_owned(),
        outcome_evaluation_performed: false,
        target_chart_audit_completed: true,
    }
}

#[cfg(test)]
mod tests;
