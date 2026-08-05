use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::{
    AnchorUpthrustResearchVariant, EmaShortResearchVariant, EmaTrendLongResearchVariant,
    ParityRuleVersion, SellClimaxBaseReclaimResearchVariant, StrictVisualBreakoutResearchVariant,
};
use std::path::PathBuf;

/// Top60 CLI 参数；部分诊断必须显式开启，默认严格要求冻结 60 个成员全覆盖。
#[derive(Debug)]
pub(super) struct Args {
    /// 可选 JSON 报告输出路径；缺省时只写标准输出。
    pub(super) output: Option<PathBuf>,
    /// 允许缺少冻结成员的诊断运行，不能作为正式 Top60 结论。
    pub(super) allow_partial_diagnostic: bool,
    /// 本次回放绑定的冻结 Pine/Rust 规则版本。
    pub(super) rule_version: ParityRuleVersion,
    /// V19 EMA 空头 Research-only 消融；默认值不改变冻结 Pine 行为。
    pub(super) ema_short_variant: EmaShortResearchVariant,
    /// V19 EMA 趋势多逐层补充来源；默认值不改变冻结 Pine 行为。
    pub(super) ema_trend_long_variant: EmaTrendLongResearchVariant,
    /// V19 保守趋势多基线上的卖压衰竭收回独立研究分支。
    pub(super) sell_climax_base_reclaim_variant: SellClimaxBaseReclaimResearchVariant,
    /// V20 扫高失败家族的锚区/确认单变量；重复 CLI 开关在一个进程内按顺序回放。
    pub(super) anchor_upthrust_variants: Vec<AnchorUpthrustResearchVariant>,
    /// Candidate V20 上严格视觉横盘上破做多家族的独立入场时序版本。
    pub(super) strict_visual_breakout_variant: StrictVisualBreakoutResearchVariant,
    /// baseline 缓存目录；缓存键仍会绑定数据、可执行文件和全部回放口径。
    pub(super) baseline_cache_dir: PathBuf,
    /// 显式关闭 baseline 缓存，供缓存实现本身的对照验证使用。
    pub(super) baseline_cache_enabled: bool,
}

/// 解析最小只读参数集，拒绝未知开关以免研究口径被隐式改变。
pub(super) fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut output = None;
    let mut allow_partial_diagnostic = false;
    // 既有无参数调用固定保留 V2；新研究版本必须显式选择，避免历史报告静默换规则。
    let mut rule_version = ParityRuleVersion::Current3cbbc9d8;
    let mut ema_short_variant = EmaShortResearchVariant::Baseline;
    let mut ema_trend_long_variant = EmaTrendLongResearchVariant::Baseline;
    let mut sell_climax_base_reclaim_variant = SellClimaxBaseReclaimResearchVariant::Baseline;
    let mut anchor_upthrust_variants = Vec::new();
    let mut strict_visual_breakout_variant = StrictVisualBreakoutResearchVariant::Baseline;
    let mut baseline_cache_dir = PathBuf::from("target/research-cache/tradingview_velocity_top60");
    let mut baseline_cache_enabled = true;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().context("--output requires a file path")?,
                ));
            }
            "--allow-partial-diagnostic" => allow_partial_diagnostic = true,
            "--ema-short-variant" => {
                ema_short_variant = parse_ema_short_variant(
                    &args
                        .next()
                        .context("--ema-short-variant requires a variant name")?,
                )?;
            }
            "--ema-trend-long-variant" => {
                ema_trend_long_variant = parse_ema_trend_long_variant(
                    &args
                        .next()
                        .context("--ema-trend-long-variant requires a variant name")?,
                )?;
            }
            "--sell-climax-base-reclaim-variant" => {
                sell_climax_base_reclaim_variant = parse_sell_climax_base_reclaim_variant(
                    &args
                        .next()
                        .context("--sell-climax-base-reclaim-variant requires baseline or v1")?,
                )?;
            }
            "--anchor-upthrust-variant" => {
                anchor_upthrust_variants.push(parse_anchor_upthrust_variant(
                    &args.next().context(
                        "--anchor-upthrust-variant requires a registered V20-V29 variant",
                    )?,
                )?);
            }
            "--strict-visual-breakout-variant" => {
                strict_visual_breakout_variant = parse_strict_visual_breakout_variant(
                    &args
                        .next()
                        .context(
                            "--strict-visual-breakout-variant requires baseline, v1, v2-retest-acceptance, v3-body-midpoint-hold, v4-short-range-32-one-r, v5-breakout-body-strength-60pct-25bps, v6-weak-departure-one-bar-probation, v8-acceptance-margin-0-40-atr, v9-external-structure-clearance, v10-symmetric-retained-breakout-25pct, v11-breakout-candle-extreme-stop, or v12-breakout-candle-extreme-stop-min-1atr",
                        )?,
                )?;
            }
            "--baseline-cache-dir" => {
                baseline_cache_dir = PathBuf::from(
                    args.next()
                        .context("--baseline-cache-dir requires a directory path")?,
                );
            }
            "--no-baseline-cache" => baseline_cache_enabled = false,
            "--rule-version" => {
                rule_version = match args
                    .next()
                    .context(
                        "--rule-version requires current-v2 or candidate-v8 through candidate-v20",
                    )?
                    .as_str()
                {
                    "current-v2" => ParityRuleVersion::Current3cbbc9d8,
                    "candidate-v8" => ParityRuleVersion::CandidateV8,
                    "candidate-v9" => ParityRuleVersion::CandidateV9,
                    "candidate-v10" => ParityRuleVersion::CandidateV10,
                    "candidate-v11" => ParityRuleVersion::CandidateV11,
                    "candidate-v12" => ParityRuleVersion::CandidateV12,
                    "candidate-v13" => ParityRuleVersion::CandidateV13,
                    "candidate-v14" => ParityRuleVersion::CandidateV14,
                    "candidate-v15" => ParityRuleVersion::CandidateV15,
                    "candidate-v16" => ParityRuleVersion::CandidateV16,
                    "candidate-v17" => ParityRuleVersion::CandidateV17,
                    "candidate-v18" => ParityRuleVersion::CandidateV18,
                    "candidate-v19" => ParityRuleVersion::CandidateV19,
                    "candidate-v20" => ParityRuleVersion::CandidateV20,
                    other => bail!("unsupported --rule-version: {other}"),
                };
            }
            "--help" | "-h" => {
                println!(
                    "Usage: tradingview_velocity_top60 [--rule-version current-v2|candidate-v8..candidate-v20] [--ema-short-variant baseline|slope-spread|structure-break|structure-break-depth-0-10-atr|structure-break-depth-0-20-atr|structure-break-ema676-falling-20|right-side-retest|distance-guard|extreme-volume-acceptance] [--ema-trend-long-variant baseline|weekly-volume-p80|weekly-p80-tp-floor-2-5|weekly-p80-tp-floor-2-5-body-0-003|weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-2-atr|weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr|weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-bullish-acceptance|weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-distance-1-35-atr|weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-distance-1-5-atr|weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-4-atr|weekly-p80-tp-floor-2-5-body-0-003-distance-1-5-atr|conservative-target-gap] [--sell-climax-base-reclaim-variant baseline|v1] [--anchor-upthrust-variant baseline|right-side-confirmation|target-consumption-cap-25|target-consumption-cap-33|target-consumption-cap-50|recent-horizontal-first-break|recent-horizontal-first-break-close-back|recent-horizontal-direction-efficiency-30|recent-horizontal-direction-efficiency-35|recent-horizontal-direction-efficiency-40|active-parent-horizontal|active-parent-horizontal-breakout-body-rejection|active-parent-horizontal-normalized-body-rejection-10pct|active-parent-horizontal-shallow-breakout-excess-10pct|active-parent-horizontal-edge-transitions-3-shallow-breakout-excess-10pct]... [--strict-visual-breakout-variant baseline|v1|v2-retest-acceptance|v3-body-midpoint-hold|v4-short-range-32-one-r|v5-breakout-body-strength-60pct-25bps|v6-weak-departure-one-bar-probation|v8-acceptance-margin-0-40-atr|v9-external-structure-clearance|v10-symmetric-retained-breakout-25pct|v11-breakout-candle-extreme-stop|v12-breakout-candle-extreme-stop-min-1atr] [--baseline-cache-dir DIR] [--no-baseline-cache] [--output PATH] [--allow-partial-diagnostic]"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    if anchor_upthrust_variants.is_empty() {
        anchor_upthrust_variants.push(AnchorUpthrustResearchVariant::Baseline);
    }
    let mut unique_anchor_upthrust_variants = Vec::new();
    for variant in anchor_upthrust_variants {
        if unique_anchor_upthrust_variants.contains(&variant) {
            bail!("duplicate --anchor-upthrust-variant: {}", variant.slug());
        }
        unique_anchor_upthrust_variants.push(variant);
    }
    if ema_short_variant != EmaShortResearchVariant::Baseline
        && rule_version != ParityRuleVersion::CandidateV19
    {
        bail!("EMA short ablations are only valid with --rule-version candidate-v19");
    }
    if ema_trend_long_variant != EmaTrendLongResearchVariant::Baseline
        && rule_version != ParityRuleVersion::CandidateV19
    {
        bail!("EMA trend long ladder is only valid with --rule-version candidate-v19");
    }
    if ema_short_variant != EmaShortResearchVariant::Baseline
        && ema_trend_long_variant != EmaTrendLongResearchVariant::Baseline
    {
        bail!("one run may change only one Research variable");
    }
    if sell_climax_base_reclaim_variant.is_enabled()
        && (rule_version != ParityRuleVersion::CandidateV19
            || ema_short_variant != EmaShortResearchVariant::Baseline
            || ema_trend_long_variant != EmaTrendLongResearchVariant::ConservativeTargetGap)
    {
        bail!("sell climax V1 requires candidate-v19, baseline EMA short, and conservative-target-gap EMA long");
    }
    if unique_anchor_upthrust_variants
        .iter()
        .copied()
        .any(AnchorUpthrustResearchVariant::is_enabled)
        && (rule_version != ParityRuleVersion::CandidateV20
            || ema_short_variant != EmaShortResearchVariant::Baseline
            || ema_trend_long_variant != EmaTrendLongResearchVariant::Baseline
            || sell_climax_base_reclaim_variant.is_enabled())
    {
        bail!("anchor upthrust V21-V30 variants require candidate-v20 and all other Research variants at baseline");
    }
    if strict_visual_breakout_variant.is_enabled()
        && (rule_version != ParityRuleVersion::CandidateV20
            || ema_short_variant != EmaShortResearchVariant::Baseline
            || ema_trend_long_variant != EmaTrendLongResearchVariant::Baseline
            || sell_climax_base_reclaim_variant.is_enabled()
            || unique_anchor_upthrust_variants
                .iter()
                .copied()
                .any(AnchorUpthrustResearchVariant::is_enabled))
    {
        bail!("strict visual breakout variants require candidate-v20 and all other Research variants at baseline");
    }
    Ok(Args {
        output,
        allow_partial_diagnostic,
        rule_version,
        ema_short_variant,
        ema_trend_long_variant,
        sell_climax_base_reclaim_variant,
        anchor_upthrust_variants: unique_anchor_upthrust_variants,
        strict_visual_breakout_variant,
        baseline_cache_dir,
        baseline_cache_enabled,
    })
}

/// 解析严格视觉横盘上破家族的独立 Research 版本。
fn parse_strict_visual_breakout_variant(
    value: &str,
) -> Result<StrictVisualBreakoutResearchVariant> {
    match value {
        "baseline" => Ok(StrictVisualBreakoutResearchVariant::Baseline),
        "v1" => Ok(StrictVisualBreakoutResearchVariant::V1),
        "v2-retest-acceptance" => Ok(StrictVisualBreakoutResearchVariant::V2RetestAcceptance),
        "v3-body-midpoint-hold" => Ok(StrictVisualBreakoutResearchVariant::V3BodyMidpointHold),
        "v4-short-range-32-one-r" => Ok(StrictVisualBreakoutResearchVariant::V4ShortRangeOneR),
        "v5-breakout-body-strength-60pct-25bps" => {
            Ok(StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength)
        }
        "v6-weak-departure-one-bar-probation" => {
            Ok(StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation)
        }
        "v8-acceptance-margin-0-40-atr" => {
            Ok(StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr)
        }
        "v9-external-structure-clearance" => {
            Ok(StrictVisualBreakoutResearchVariant::V9ExternalStructureClearance)
        }
        "v10-symmetric-retained-breakout-25pct" => {
            Ok(StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout)
        }
        "v11-breakout-candle-extreme-stop" => {
            Ok(StrictVisualBreakoutResearchVariant::V11BreakoutCandleExtremeStop)
        }
        "v12-breakout-candle-extreme-stop-min-1atr" => {
            Ok(StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr)
        }
        other => bail!("unsupported --strict-visual-breakout-variant: {other}"),
    }
}

/// 解析 V20 扫高失败家族的独立锚区或确认时序，拒绝未登记组合。
fn parse_anchor_upthrust_variant(value: &str) -> Result<AnchorUpthrustResearchVariant> {
    match value {
        "baseline" => Ok(AnchorUpthrustResearchVariant::Baseline),
        "right-side-confirmation" => Ok(AnchorUpthrustResearchVariant::RightSideConfirmation),
        "target-consumption-cap-25" => Ok(AnchorUpthrustResearchVariant::TargetConsumptionCap25),
        "target-consumption-cap-33" => Ok(AnchorUpthrustResearchVariant::TargetConsumptionCap33),
        "target-consumption-cap-50" => Ok(AnchorUpthrustResearchVariant::TargetConsumptionCap50),
        "recent-horizontal-first-break" => {
            Ok(AnchorUpthrustResearchVariant::RecentHorizontalFirstBreak)
        }
        "recent-horizontal-first-break-close-back" => {
            Ok(AnchorUpthrustResearchVariant::RecentHorizontalFirstBreakCloseBack)
        }
        "recent-horizontal-direction-efficiency-30" => {
            Ok(AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency30)
        }
        "recent-horizontal-direction-efficiency-35" => {
            Ok(AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency35)
        }
        "recent-horizontal-direction-efficiency-40" => {
            Ok(AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency40)
        }
        "active-parent-horizontal" => Ok(AnchorUpthrustResearchVariant::ActiveParentHorizontal),
        "active-parent-horizontal-breakout-body-rejection" => {
            Ok(AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection)
        }
        "active-parent-horizontal-normalized-body-rejection-10pct" => {
            Ok(AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct)
        }
        "active-parent-horizontal-shallow-breakout-excess-10pct" => {
            Ok(AnchorUpthrustResearchVariant::ActiveParentHorizontalShallowBreakoutExcess10Pct)
        }
        "active-parent-horizontal-edge-transitions-3-shallow-breakout-excess-10pct" => Ok(
            AnchorUpthrustResearchVariant::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
        ),
        other => bail!("unsupported --anchor-upthrust-variant: {other}"),
    }
}

/// 解析卖压衰竭独立分支，拒绝未登记的组合版本。
fn parse_sell_climax_base_reclaim_variant(
    value: &str,
) -> Result<SellClimaxBaseReclaimResearchVariant> {
    match value {
        "baseline" => Ok(SellClimaxBaseReclaimResearchVariant::Baseline),
        "v1" => Ok(SellClimaxBaseReclaimResearchVariant::V1),
        other => bail!("unsupported --sell-climax-base-reclaim-variant: {other}"),
    }
}

/// 把稳定 CLI 名称映射为单变量枚举，拒绝未登记的研究假设。
fn parse_ema_short_variant(value: &str) -> Result<EmaShortResearchVariant> {
    match value {
        "baseline" => Ok(EmaShortResearchVariant::Baseline),
        "slope-spread" => Ok(EmaShortResearchVariant::SlopeSpread),
        "structure-break" => Ok(EmaShortResearchVariant::StructureBreak),
        "structure-break-depth-0-10-atr" => Ok(EmaShortResearchVariant::StructureBreakDepth10),
        "structure-break-depth-0-20-atr" => Ok(EmaShortResearchVariant::StructureBreakDepth20),
        "structure-break-ema676-falling-20" => {
            Ok(EmaShortResearchVariant::StructureBreakEma676Falling20)
        }
        "right-side-retest" => Ok(EmaShortResearchVariant::RightSideRetest),
        "distance-guard" => Ok(EmaShortResearchVariant::DistanceGuard),
        "extreme-volume-acceptance" => Ok(EmaShortResearchVariant::ExtremeVolumeAcceptance),
        other => bail!("unsupported --ema-short-variant: {other}"),
    }
}

/// 把稳定 CLI 名称映射为 EMA 趋势多逐层门槛，禁止跳过未登记组合。
fn parse_ema_trend_long_variant(value: &str) -> Result<EmaTrendLongResearchVariant> {
    match value {
        "baseline" => Ok(EmaTrendLongResearchVariant::Baseline),
        "weekly-volume-p80" => Ok(EmaTrendLongResearchVariant::WeeklyVolumeP80),
        "weekly-p80-tp-floor-2-5" => Ok(EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25),
        "weekly-p80-tp-floor-2-5-body-0-003" => {
            Ok(EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003)
        }
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-2-atr" => {
            Ok(EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth20)
        }
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr" => {
            Ok(EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30)
        }
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-bullish-acceptance" => Ok(
            EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30BullishAcceptance,
        ),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-distance-1-35-atr" => Ok(
            EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance135,
        ),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-3-atr-distance-1-5-atr" => Ok(
            EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth30Distance150,
        ),
        "weekly-p80-tp-floor-2-5-body-0-003-break-depth-0-4-atr" => {
            Ok(EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003BreakDepth40)
        }
        "weekly-p80-tp-floor-2-5-body-0-003-distance-1-5-atr" => {
            Ok(EmaTrendLongResearchVariant::WeeklyP80TakeProfitFloor25Body003Distance15)
        }
        "conservative-target-gap" => Ok(EmaTrendLongResearchVariant::ConservativeTargetGap),
        other => bail!("unsupported --ema-trend-long-variant: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_anchor_variants_keep_declaration_order() {
        let parsed = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "baseline".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "right-side-confirmation".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "target-consumption-cap-25".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "recent-horizontal-first-break".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "recent-horizontal-first-break-close-back".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "recent-horizontal-direction-efficiency-30".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "recent-horizontal-direction-efficiency-35".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "recent-horizontal-direction-efficiency-40".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "active-parent-horizontal".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "active-parent-horizontal-breakout-body-rejection".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "active-parent-horizontal-normalized-body-rejection-10pct".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "active-parent-horizontal-shallow-breakout-excess-10pct".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "active-parent-horizontal-edge-transitions-3-shallow-breakout-excess-10pct".to_owned(),
        ])
        .expect("registered batch");

        assert_eq!(
            parsed.anchor_upthrust_variants,
            vec![
                AnchorUpthrustResearchVariant::Baseline,
                AnchorUpthrustResearchVariant::RightSideConfirmation,
                AnchorUpthrustResearchVariant::TargetConsumptionCap25,
                AnchorUpthrustResearchVariant::RecentHorizontalFirstBreak,
                AnchorUpthrustResearchVariant::RecentHorizontalFirstBreakCloseBack,
                AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency30,
                AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency35,
                AnchorUpthrustResearchVariant::RecentHorizontalDirectionEfficiency40,
                AnchorUpthrustResearchVariant::ActiveParentHorizontal,
                AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection,
                AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
                AnchorUpthrustResearchVariant::ActiveParentHorizontalShallowBreakoutExcess10Pct,
                AnchorUpthrustResearchVariant::ActiveParentHorizontalEdgeTransitions3ShallowBreakoutExcess10Pct,
            ]
        );
    }

    #[test]
    fn duplicate_anchor_variant_is_rejected() {
        let error = parse_args([
            "--anchor-upthrust-variant".to_owned(),
            "baseline".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "baseline".to_owned(),
        ])
        .expect_err("duplicate must fail");

        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn strict_visual_breakout_variants_are_bound_to_candidate_v20_as_the_only_research_axis() {
        let parsed = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v1".to_owned(),
        ])
        .expect("registered strict visual variant");
        assert_eq!(
            parsed.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V1
        );

        let parsed_v2 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v2-retest-acceptance".to_owned(),
        ])
        .expect("registered strict visual V2 variant");
        assert_eq!(
            parsed_v2.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V2RetestAcceptance
        );

        let parsed_v3 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v3-body-midpoint-hold".to_owned(),
        ])
        .expect("registered strict visual V3 variant");
        assert_eq!(
            parsed_v3.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V3BodyMidpointHold
        );

        let parsed_v4 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v4-short-range-32-one-r".to_owned(),
        ])
        .expect("registered strict visual V4 variant");
        assert_eq!(
            parsed_v4.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V4ShortRangeOneR
        );

        let parsed_v5 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v5-breakout-body-strength-60pct-25bps".to_owned(),
        ])
        .expect("registered strict visual V5 variant");
        assert_eq!(
            parsed_v5.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V5BreakoutBodyStrength
        );

        let parsed_v6 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v6-weak-departure-one-bar-probation".to_owned(),
        ])
        .expect("registered strict visual V6 variant");
        assert_eq!(
            parsed_v6.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V6WeakDepartureProbation
        );

        let parsed_v8 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v8-acceptance-margin-0-40-atr".to_owned(),
        ])
        .expect("registered strict visual V8 variant");
        assert_eq!(
            parsed_v8.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V8AcceptanceMargin40Atr
        );

        let parsed_v9 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v9-external-structure-clearance".to_owned(),
        ])
        .expect("registered strict visual V9 variant");
        assert_eq!(
            parsed_v9.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V9ExternalStructureClearance
        );

        let parsed_v10 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v10-symmetric-retained-breakout-25pct".to_owned(),
        ])
        .expect("registered strict visual symmetric contract");
        assert_eq!(
            parsed_v10.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V10SymmetricRetainedBreakout
        );

        let parsed_v11 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v11-breakout-candle-extreme-stop".to_owned(),
        ])
        .expect("registered strict visual breakout-candle stop contract");
        assert_eq!(
            parsed_v11.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V11BreakoutCandleExtremeStop
        );

        let parsed_v12 = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v12-breakout-candle-extreme-stop-min-1atr".to_owned(),
        ])
        .expect("registered strict visual minimum-ATR stop contract");
        assert_eq!(
            parsed_v12.strict_visual_breakout_variant,
            StrictVisualBreakoutResearchVariant::V12ExtremeStopMinOneAtr
        );

        let error = parse_args([
            "--rule-version".to_owned(),
            "candidate-v20".to_owned(),
            "--strict-visual-breakout-variant".to_owned(),
            "v1".to_owned(),
            "--anchor-upthrust-variant".to_owned(),
            "active-parent-horizontal".to_owned(),
        ])
        .expect_err("two research axes must fail closed");
        assert!(error.to_string().contains("strict visual breakout"));
    }

    #[test]
    fn baseline_cache_is_enabled_unless_explicitly_disabled() {
        assert!(
            parse_args(Vec::<String>::new())
                .expect("defaults")
                .baseline_cache_enabled
        );
        assert!(
            !parse_args(["--no-baseline-cache".to_owned()])
                .expect("disabled cache")
                .baseline_cache_enabled
        );
    }
}
