/// 破位做空从 paper 观察推进到 live cutover 前必须满足的样本门槛。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarketVelocityPromotionCriteria {
    /// 最少有效交易样本数。
    pub min_trade_samples: usize,
    /// 最低胜率百分比。
    pub min_win_rate_pct: f64,
    /// 最大 paper R 曲线回撤百分比。
    pub max_drawdown_pct: f64,
    /// 最低总 R，通常要求为正。
    pub min_total_r: f64,
}

/// Web paper outcome 表中推进评估所需的最小字段投影。
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityPaperOutcomeSample {
    /// paper 评估目标 R。
    pub target_r: f64,
    /// paper 评估持仓窗口小时数。
    pub horizon_hours: i32,
    /// 结果状态，例如 win、loss、timeout。
    pub outcome_status: String,
    /// 以 R 表示的 paper 结果；为空的未决样本不计入交易样本。
    pub result_r: Option<f64>,
}

/// 破位做空 paper readiness 报告；只表达是否可进入下一步 cutover 评审，不触发实盘。
#[derive(Debug, Clone, PartialEq)]
pub struct MarketVelocityPromotionReadinessReport {
    /// 被评估的目标 R 桶。
    pub target_r: f64,
    /// 被评估的持仓窗口小时数。
    pub horizon_hours: i32,
    /// 有效交易样本数。
    pub trade_samples: usize,
    /// 胜利样本数。
    pub wins: usize,
    /// 失败样本数。
    pub losses: usize,
    /// 胜率百分比。
    pub win_rate_pct: f64,
    /// paper 总 R。
    pub total_r: f64,
    /// paper R 曲线最大回撤百分比。
    pub max_drawdown_pct: f64,
    /// 状态值：paper_ready 或 blocked。
    pub production_status: String,
    /// 阻断原因列表。
    pub blockers: Vec<String>,
    /// 是否允许进入 live cutover 后续评审；这不是实盘下单开关。
    pub promotion_review_ready: bool,
}

/// 根据已落库的 paper outcome 评估破位做空是否具备 live cutover 评审条件。
/// 调用方如果同时传入多个 target/horizon，应使用 select_breakdown_short_paper_readiness_bucket。
pub fn evaluate_breakdown_short_paper_readiness(
    outcomes: &[MarketVelocityPaperOutcomeSample],
    criteria: MarketVelocityPromotionCriteria,
) -> MarketVelocityPromotionReadinessReport {
    let target_r = outcomes.first().map_or(0.0, |outcome| outcome.target_r);
    let horizon_hours = outcomes.first().map_or(0, |outcome| outcome.horizon_hours);
    evaluate_breakdown_short_paper_readiness_bucket(outcomes, criteria, target_r, horizon_hours)
}

/// 从多个 target/horizon paper outcome 桶中选择最接近 promotion review 的候选桶。
pub fn select_breakdown_short_paper_readiness_bucket(
    outcomes: &[MarketVelocityPaperOutcomeSample],
    criteria: MarketVelocityPromotionCriteria,
) -> Option<MarketVelocityPromotionReadinessReport> {
    let mut keys = Vec::new();
    for outcome in outcomes {
        if !keys.iter().any(|(target_r, horizon_hours)| {
            same_target(*target_r, outcome.target_r) && *horizon_hours == outcome.horizon_hours
        }) {
            keys.push((outcome.target_r, outcome.horizon_hours));
        }
    }
    keys.into_iter()
        .map(|(target_r, horizon_hours)| {
            let bucket_outcomes = outcomes
                .iter()
                .filter(|outcome| {
                    same_target(outcome.target_r, target_r)
                        && outcome.horizon_hours == horizon_hours
                })
                .cloned()
                .collect::<Vec<_>>();
            evaluate_breakdown_short_paper_readiness_bucket(
                &bucket_outcomes,
                criteria,
                target_r,
                horizon_hours,
            )
        })
        .max_by(compare_readiness_report)
}

fn evaluate_breakdown_short_paper_readiness_bucket(
    outcomes: &[MarketVelocityPaperOutcomeSample],
    criteria: MarketVelocityPromotionCriteria,
    target_r: f64,
    horizon_hours: i32,
) -> MarketVelocityPromotionReadinessReport {
    let trade_results: Vec<f64> = outcomes
        .iter()
        .filter_map(|outcome| {
            outcome.result_r.filter(|_| {
                matches!(
                    outcome.outcome_status.trim().to_ascii_lowercase().as_str(),
                    "win" | "loss" | "timeout" | "flat"
                )
            })
        })
        .collect();
    let trade_samples = trade_results.len();
    let wins = trade_results
        .iter()
        .filter(|result_r| **result_r > 0.0)
        .count();
    let losses = trade_results
        .iter()
        .filter(|result_r| **result_r <= 0.0)
        .count();
    let total_r = round_metric(trade_results.iter().sum());
    let win_rate_pct = if trade_samples == 0 {
        0.0
    } else {
        round_metric(wins as f64 * 100.0 / trade_samples as f64)
    };
    let max_drawdown_pct = max_drawdown_pct_from_r(&trade_results);
    let mut blockers = Vec::new();
    if trade_samples < criteria.min_trade_samples {
        blockers.push("paper_trade_samples_below_minimum".to_string());
    }
    if win_rate_pct < criteria.min_win_rate_pct {
        blockers.push("paper_win_rate_below_minimum".to_string());
    }
    if max_drawdown_pct > criteria.max_drawdown_pct {
        blockers.push("paper_max_drawdown_above_limit".to_string());
    }
    if total_r <= criteria.min_total_r {
        blockers.push("paper_total_r_not_positive".to_string());
    }
    let promotion_review_ready = blockers.is_empty();
    MarketVelocityPromotionReadinessReport {
        target_r,
        horizon_hours,
        trade_samples,
        wins,
        losses,
        win_rate_pct,
        total_r,
        max_drawdown_pct,
        production_status: if promotion_review_ready {
            "paper_ready".to_string()
        } else {
            "blocked".to_string()
        },
        blockers,
        promotion_review_ready,
    }
}

fn compare_readiness_report(
    left: &MarketVelocityPromotionReadinessReport,
    right: &MarketVelocityPromotionReadinessReport,
) -> std::cmp::Ordering {
    right
        .blockers
        .len()
        .cmp(&left.blockers.len())
        .then_with(|| {
            left.total_r
                .partial_cmp(&right.total_r)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            left.win_rate_pct
                .partial_cmp(&right.win_rate_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| left.trade_samples.cmp(&right.trade_samples))
}

fn same_target(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.0001
}

fn max_drawdown_pct_from_r(results: &[f64]) -> f64 {
    let mut equity = 100.0;
    let mut peak = equity;
    let mut max_drawdown_pct = 0.0;
    for result in results {
        equity += result;
        if equity > peak {
            peak = equity;
        }
        if peak > 0.0 {
            let drawdown_pct = (peak - equity) / peak * 100.0;
            if drawdown_pct > max_drawdown_pct {
                max_drawdown_pct = drawdown_pct;
            }
        }
    }
    round_metric(max_drawdown_pct)
}

fn round_metric(value: f64) -> f64 {
    (value * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakdown_short_paper_readiness_blocks_small_or_losing_samples() {
        let outcomes = vec![
            paper_outcome("win", 1.0),
            paper_outcome("loss", -1.0),
            paper_outcome("loss", -1.0),
        ];

        let report = evaluate_breakdown_short_paper_readiness(
            &outcomes,
            MarketVelocityPromotionCriteria {
                min_trade_samples: 10,
                min_win_rate_pct: 60.0,
                max_drawdown_pct: 15.0,
                min_total_r: 0.0,
            },
        );

        assert_eq!(report.production_status, "blocked");
        assert!(!report.promotion_review_ready);
        assert_eq!(report.trade_samples, 3);
        assert!(report
            .blockers
            .contains(&"paper_trade_samples_below_minimum".to_string()));
        assert!(report
            .blockers
            .contains(&"paper_win_rate_below_minimum".to_string()));
        assert!(report
            .blockers
            .contains(&"paper_total_r_not_positive".to_string()));
    }

    #[test]
    fn breakdown_short_paper_readiness_passes_thresholds_for_promotion_review() {
        let mut outcomes = Vec::new();
        outcomes.extend((0..6).map(|_| paper_outcome("win", 1.0)));
        outcomes.extend((0..4).map(|_| paper_outcome("loss", -1.0)));

        let report = evaluate_breakdown_short_paper_readiness(
            &outcomes,
            MarketVelocityPromotionCriteria {
                min_trade_samples: 10,
                min_win_rate_pct: 60.0,
                max_drawdown_pct: 15.0,
                min_total_r: 0.0,
            },
        );

        assert_eq!(report.production_status, "paper_ready");
        assert!(report.promotion_review_ready);
        assert_eq!(report.trade_samples, 10);
        assert_eq!(report.win_rate_pct, 60.0);
        assert_eq!(report.total_r, 2.0);
        assert!(report.max_drawdown_pct > 0.0);
        assert!(report.blockers.is_empty());
    }

    #[test]
    fn breakdown_short_readiness_selects_best_target_horizon_bucket_without_mixing_outcomes() {
        let mut outcomes = Vec::new();
        outcomes.extend((0..8).map(|_| bucketed_paper_outcome(1.0, 24, "win", 1.0)));
        outcomes.extend((0..7).map(|_| bucketed_paper_outcome(1.0, 24, "loss", -1.0)));
        outcomes.extend((0..7).map(|_| bucketed_paper_outcome(1.0, 48, "win", 1.0)));
        outcomes.extend((0..8).map(|_| bucketed_paper_outcome(1.0, 48, "loss", -1.0)));

        let report = select_breakdown_short_paper_readiness_bucket(
            &outcomes,
            MarketVelocityPromotionCriteria {
                min_trade_samples: 10,
                min_win_rate_pct: 50.0,
                max_drawdown_pct: 15.0,
                min_total_r: 0.0,
            },
        )
        .expect("best bucket");

        assert_eq!(report.target_r, 1.0);
        assert_eq!(report.horizon_hours, 24);
        assert_eq!(report.trade_samples, 15);
        assert_eq!(report.wins, 8);
        assert_eq!(report.losses, 7);
        assert_eq!(report.total_r, 1.0);
        assert!(report.promotion_review_ready);
    }

    fn paper_outcome(status: &str, result_r: f64) -> MarketVelocityPaperOutcomeSample {
        MarketVelocityPaperOutcomeSample {
            target_r: 1.0,
            horizon_hours: 24,
            outcome_status: status.to_string(),
            result_r: Some(result_r),
        }
    }

    fn bucketed_paper_outcome(
        target_r: f64,
        horizon_hours: i32,
        status: &str,
        result_r: f64,
    ) -> MarketVelocityPaperOutcomeSample {
        MarketVelocityPaperOutcomeSample {
            target_r,
            horizon_hours,
            outcome_status: status.to_string(),
            result_r: Some(result_r),
        }
    }
}
