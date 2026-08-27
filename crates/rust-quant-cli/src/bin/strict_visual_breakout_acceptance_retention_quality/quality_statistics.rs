//! 确认保留率的覆盖、结构质量和事件簇配对统计。

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, TimeZone, Utc};

use super::*;

/// 判断账本行是否属于一个冻结阈值、时间窗及可选方向。
fn row_matches(
    row: &CandidateLedgerRow,
    threshold: f64,
    split: WindowSplit,
    direction: Option<Direction>,
) -> bool {
    row.split == split
        && row.acceptance_retention_ratio + RETENTION_EPSILON >= threshold
        && direction.is_none_or(|value| row.direction == value)
}

/// 毫秒时间戳转上海自然月标签，用于验证跨月覆盖。
fn shanghai_month(timestamp_ms: i64) -> String {
    let Some(time) = Utc
        .timestamp_millis_opt(timestamp_ms + SHANGHAI_OFFSET_MS)
        .single()
    else {
        return "invalid".to_owned();
    };
    format!("{:04}-{:02}", time.year(), time.month())
}

/// 汇总一个阈值在指定时间窗的候选覆盖；不读取未来标签。
fn coverage_stats(
    rows: &[CandidateLedgerRow],
    threshold: f64,
    split: WindowSplit,
) -> CoverageStats {
    let matching: Vec<&CandidateLedgerRow> = rows
        .iter()
        .filter(|row| row_matches(row, threshold, split, None))
        .collect();
    let symbols: BTreeSet<&str> = matching.iter().map(|row| row.symbol.as_str()).collect();
    let months: BTreeSet<String> = matching
        .iter()
        .map(|row| shanghai_month(row.signal_time_ms))
        .collect();
    let clusters: BTreeSet<&str> = matching
        .iter()
        .map(|row| row.event_cluster.as_str())
        .collect();
    CoverageStats {
        candidates: matching.len(),
        long_candidates: matching
            .iter()
            .filter(|row| row.direction == Direction::Long)
            .count(),
        short_candidates: matching
            .iter()
            .filter(|row| row.direction == Direction::Short)
            .count(),
        symbols: symbols.len(),
        shanghai_months: months.len(),
        direction_hour_event_clusters: clusters.len(),
    }
}

/// 按预注册的验证窗样本与分散度下限生成 L1 门禁结果。
pub(super) fn threshold_coverage_report(
    rows: &[CandidateLedgerRow],
    threshold: f64,
) -> ThresholdCoverageReport {
    let discovery = coverage_stats(rows, threshold, WindowSplit::Discovery);
    let validation = coverage_stats(rows, threshold, WindowSplit::Validation);
    let mut failures = Vec::new();
    if validation.candidates < 500 {
        failures.push(format!(
            "validation_candidates={} < 500",
            validation.candidates
        ));
    }
    if validation.symbols < 30 {
        failures.push(format!("validation_symbols={} < 30", validation.symbols));
    }
    if validation.shanghai_months < 5 {
        failures.push(format!(
            "validation_shanghai_months={} < 5",
            validation.shanghai_months
        ));
    }
    if validation.direction_hour_event_clusters < 100 {
        failures.push(format!(
            "validation_event_clusters={} < 100",
            validation.direction_hour_event_clusters
        ));
    }
    if validation.long_candidates < 200 {
        failures.push(format!(
            "validation_long_candidates={} < 200",
            validation.long_candidates
        ));
    }
    if validation.short_candidates < 200 {
        failures.push(format!(
            "validation_short_candidates={} < 200",
            validation.short_candidates
        ));
    }
    ThresholdCoverageReport {
        minimum_retention_ratio: threshold,
        discovery,
        validation,
        validation_gate_passed: failures.is_empty(),
        gate_failures: failures,
    }
}

/// 计算唯一变量的固定分位摘要；分位点不用于生成候选阈值。
pub(super) fn retention_distribution(rows: &[CandidateLedgerRow]) -> RetentionDistribution {
    let mut values: Vec<f64> = rows
        .iter()
        .map(|row| row.acceptance_retention_ratio)
        .filter(|value| value.is_finite())
        .collect();
    values.sort_by(f64::total_cmp);
    RetentionDistribution {
        count: values.len(),
        minimum: quantile(&values, 0.0),
        p10: quantile(&values, 0.10),
        p25: quantile(&values, 0.25),
        median: quantile(&values, 0.50),
        p75: quantile(&values, 0.75),
        p90: quantile(&values, 0.90),
        p95: quantile(&values, 0.95),
        maximum: quantile(&values, 1.0),
    }
}

/// 对排序数组做线性插值分位数；空数组返回 0。
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let position = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let weight = position - lower as f64;
        sorted[lower] * (1.0 - weight) + sorted[upper] * weight
    }
}

/// 对指定阈值、窗口和方向汇总结构质量标签。
fn quality_stats(
    rows: &[CandidateLedgerRow],
    threshold: f64,
    split: WindowSplit,
    direction: Option<Direction>,
) -> QualityStats {
    let matching: Vec<&CandidateLedgerRow> = rows
        .iter()
        .filter(|row| row_matches(row, threshold, split, direction))
        .collect();
    let count = |outcome| {
        matching
            .iter()
            .filter(|row| row.quality_outcome == Some(outcome))
            .count()
    };
    let quality = count(QualityOutcome::Quality);
    let failed = count(QualityOutcome::FailedCloseBack);
    let unresolved = count(QualityOutcome::Unresolved);
    let incomplete = count(QualityOutcome::ForwardIncomplete);
    let invalid = count(QualityOutcome::InvalidEvidence);
    let resolved = quality + failed;
    let forward_complete = quality + failed + unresolved;
    let ambiguous = matching
        .iter()
        .filter(|row| {
            row.quality_outcome == Some(QualityOutcome::FailedCloseBack)
                && row.same_bar_ambiguous == Some(true)
        })
        .count();
    QualityStats {
        candidates: matching.len(),
        forward_complete,
        resolved,
        quality,
        failed_close_back: failed,
        unresolved,
        forward_incomplete: incomplete,
        invalid_evidence: invalid,
        same_bar_ambiguous_failures: ambiguous,
        quality_rate_resolved_percent: percent(quality, resolved),
        quality_rate_complete_percent: percent(quality, forward_complete),
        quality_rate_resolved_wilson_95: wilson_95(quality, resolved),
    }
}

/// 同时生成全体及多空镜像统计，避免总样本改善掩盖单方向恶化。
pub(super) fn quality_window_report(
    rows: &[CandidateLedgerRow],
    threshold: f64,
    split: WindowSplit,
) -> QualityWindowReport {
    QualityWindowReport {
        all: quality_stats(rows, threshold, split, None),
        long: quality_stats(rows, threshold, split, Some(Direction::Long)),
        short: quality_stats(rows, threshold, split, Some(Direction::Short)),
    }
}

/// 计算 Wilson 95% 区间，样本为空时返回 0～0，避免伪造精度。
fn wilson_95(successes: usize, total: usize) -> ConfidenceInterval {
    if total == 0 {
        return ConfidenceInterval {
            lower_percent: 0.0,
            upper_percent: 0.0,
        };
    }
    let n = total as f64;
    let p = successes as f64 / n;
    let z = 1.96_f64;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let margin = z / denominator * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt());
    ConfidenceInterval {
        lower_percent: (center - margin).max(0.0) * 100.0,
        upper_percent: (center + margin).min(1.0) * 100.0,
    }
}

/// 百分比工具；分母为零时返回 0，调用方同时保留原始计数。
fn percent(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

/// 计算一个事件簇内指定阈值的已解决质量率。
fn cluster_quality_rate(rows: &[&CandidateLedgerRow], threshold: f64) -> Option<f64> {
    let mut quality = 0_usize;
    let mut resolved = 0_usize;
    for row in rows {
        if row.acceptance_retention_ratio + RETENTION_EPSILON < threshold {
            continue;
        }
        match row.quality_outcome {
            Some(QualityOutcome::Quality) => {
                quality += 1;
                resolved += 1;
            }
            Some(QualityOutcome::FailedCloseBack) => resolved += 1,
            _ => {}
        }
    }
    (resolved > 0).then_some(quality as f64 / resolved as f64 * 100.0)
}

/// 对共同出现已解决样本的事件簇做配对差值，降低市场共振造成的伪样本量。
fn paired_cluster_lift(
    rows: &[CandidateLedgerRow],
    split: WindowSplit,
    candidate_threshold: f64,
) -> PairedClusterLift {
    let mut clusters: BTreeMap<&str, Vec<&CandidateLedgerRow>> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.split == split) {
        clusters
            .entry(row.event_cluster.as_str())
            .or_default()
            .push(row);
    }
    let differences: Vec<f64> = clusters
        .values()
        .filter_map(|cluster| {
            let baseline = cluster_quality_rate(cluster, RETENTION_THRESHOLDS[0])?;
            let candidate = cluster_quality_rate(cluster, candidate_threshold)?;
            Some(candidate - baseline)
        })
        .collect();
    if differences.is_empty() {
        return PairedClusterLift {
            paired_clusters: 0,
            mean_lift_percent_points: 0.0,
            standard_error_percent_points: 0.0,
            approximate_95_lower_percent_points: 0.0,
            approximate_95_upper_percent_points: 0.0,
        };
    }
    let mean = differences.iter().sum::<f64>() / differences.len() as f64;
    let standard_error = if differences.len() > 1 {
        let variance = differences
            .iter()
            .map(|difference| (difference - mean).powi(2))
            .sum::<f64>()
            / (differences.len() - 1) as f64;
        variance.sqrt() / (differences.len() as f64).sqrt()
    } else {
        0.0
    };
    PairedClusterLift {
        paired_clusters: differences.len(),
        mean_lift_percent_points: mean,
        standard_error_percent_points: standard_error,
        approximate_95_lower_percent_points: mean - 1.96 * standard_error,
        approximate_95_upper_percent_points: mean + 1.96 * standard_error,
    }
}

/// 依据预注册条件生成一个阈值的 L2 报告；阈值选择顺序在主流程中固定。
pub(super) fn l2_threshold_report(
    rows: &[CandidateLedgerRow],
    threshold: f64,
    baseline_discovery: &QualityWindowReport,
    baseline_validation: &QualityWindowReport,
) -> L2ThresholdReport {
    let coverage = threshold_coverage_report(rows, threshold);
    let discovery = quality_window_report(rows, threshold, WindowSplit::Discovery);
    let validation = quality_window_report(rows, threshold, WindowSplit::Validation);
    let discovery_lift = discovery.all.quality_rate_resolved_percent
        - baseline_discovery.all.quality_rate_resolved_percent;
    let validation_lift = validation.all.quality_rate_resolved_percent
        - baseline_validation.all.quality_rate_resolved_percent;
    let long_lift = validation.long.quality_rate_resolved_percent
        - baseline_validation.long.quality_rate_resolved_percent;
    let short_lift = validation.short.quality_rate_resolved_percent
        - baseline_validation.short.quality_rate_resolved_percent;
    let cluster_lift = paired_cluster_lift(rows, WindowSplit::Validation, threshold);
    let mut failures = Vec::new();
    if !coverage.validation_gate_passed {
        failures.push("l1_validation_coverage_failed".to_owned());
    }
    if discovery_lift < 3.0 {
        failures.push(format!("discovery_lift={discovery_lift:.6}pp < 3pp"));
    }
    if validation_lift < 5.0 {
        failures.push(format!("validation_lift={validation_lift:.6}pp < 5pp"));
    }
    if long_lift < 2.0 {
        failures.push(format!("validation_long_lift={long_lift:.6}pp < 2pp"));
    }
    if short_lift < 2.0 {
        failures.push(format!("validation_short_lift={short_lift:.6}pp < 2pp"));
    }
    if cluster_lift.approximate_95_lower_percent_points <= 0.0 {
        failures.push(format!(
            "validation_cluster_lift_95_lower={:.6}pp <= 0",
            cluster_lift.approximate_95_lower_percent_points
        ));
    }
    L2ThresholdReport {
        minimum_retention_ratio: threshold,
        coverage,
        discovery,
        validation,
        discovery_lift_vs_baseline_percent_points: discovery_lift,
        validation_lift_vs_baseline_percent_points: validation_lift,
        validation_long_lift_vs_baseline_percent_points: long_lift,
        validation_short_lift_vs_baseline_percent_points: short_lift,
        validation_paired_event_cluster_lift: cluster_lift,
        joint_gate_passed: failures.is_empty(),
        joint_gate_failures: failures,
    }
}
