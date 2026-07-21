use super::*;

/// 构造预注册时间段、极端/对照与方向面板并判断晋级门槛。
pub(super) fn build_report(
    schedule: &UniverseSchedule,
    okx_symbols: usize,
    binance_audit: BinanceKlineAudit,
    stages: CrossExchangeBasisStages,
    observations: &[BasisObservation],
) -> CrossExchangeBasisPanelReport {
    let split_ms = schedule.windows[6].from_ms;
    let extreme = |value: &&BasisObservation| value.z_score.abs() >= EXTREME_Z;
    let control = |value: &&BasisObservation| value.z_score.abs() < EXTREME_Z;
    let extreme_all = observations.iter().filter(extreme).collect::<Vec<_>>();
    let control_all = observations.iter().filter(control).collect::<Vec<_>>();
    let extreme_discovery_values = extreme_all
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_discovery_values = control_all
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let extreme_validation_values = extreme_all
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_validation_values = control_all
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let extreme_positive_z_values = extreme_all
        .iter()
        .filter(|value| value.z_score > 0.0)
        .copied()
        .collect::<Vec<_>>();
    let extreme_negative_z_values = extreme_all
        .iter()
        .filter(|value| value.z_score < 0.0)
        .copied()
        .collect::<Vec<_>>();
    let extreme_overall = summarize(&extreme_all);
    let control_overall = summarize(&control_all);
    let extreme_discovery = summarize(&extreme_discovery_values);
    let control_discovery = summarize(&control_discovery_values);
    let extreme_validation = summarize(&extreme_validation_values);
    let control_validation = summarize(&control_validation_values);
    let extreme_positive_z = summarize(&extreme_positive_z_values);
    let extreme_negative_z = summarize(&extreme_negative_z_values);
    let factor_gate_passed = extreme_overall.observations >= 300
        && extreme_discovery.observations >= 100
        && extreme_validation.observations >= 100
        && extreme_positive_z.observations >= 100
        && extreme_negative_z.observations >= 100
        && segment_passed(&extreme_discovery, &control_discovery)
        && segment_passed(&extreme_validation, &control_validation)
        && extreme_positive_z
            .mean_forward_4h
            .is_some_and(|value| value > 0.0032)
        && extreme_negative_z
            .mean_forward_4h
            .is_some_and(|value| value > 0.0032)
        && extreme_overall
            .mean_forward_1h
            .is_some_and(|value| value > 0.0)
        && extreme_overall
            .mean_forward_24h
            .is_some_and(|value| value > 0.0);
    CrossExchangeBasisPanelReport {
        rule_version: RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        okx_symbols,
        binance_audit,
        stages,
        extreme_overall,
        control_overall,
        extreme_discovery,
        control_discovery,
        extreme_validation,
        control_validation,
        extreme_positive_z,
        extreme_negative_z,
        factor_gate_passed,
    }
}

/// 构造 50bps 首次越界的时间段、方向、对照与有效事件报告。
pub(super) fn build_dislocation_report(
    schedule: &UniverseSchedule,
    okx_symbols: usize,
    binance_audit: BinanceKlineAudit,
    stages: CrossExchangeDislocationStages,
    observations: &[DislocationObservation],
) -> CrossExchangeDislocationPanelReport {
    let split_ms = schedule.windows[6].from_ms;
    let executable_all = observations
        .iter()
        .filter(|value| value.executable)
        .collect::<Vec<_>>();
    let control_all = observations
        .iter()
        .filter(|value| !value.executable)
        .collect::<Vec<_>>();
    let executable_discovery_values = executable_all
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_discovery_values = control_all
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let executable_validation_values = executable_all
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_validation_values = control_all
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let executable_positive_values = executable_all
        .iter()
        .filter(|value| value.deviation > 0.0)
        .copied()
        .collect::<Vec<_>>();
    let executable_negative_values = executable_all
        .iter()
        .filter(|value| value.deviation < 0.0)
        .copied()
        .collect::<Vec<_>>();
    let effective_events_4h = effective_event_count_4h(&executable_all);
    let executable_overall = summarize_dislocations(&executable_all);
    let control_overall = summarize_dislocations(&control_all);
    let executable_discovery = summarize_dislocations(&executable_discovery_values);
    let control_discovery = summarize_dislocations(&control_discovery_values);
    let executable_validation = summarize_dislocations(&executable_validation_values);
    let control_validation = summarize_dislocations(&control_validation_values);
    let executable_positive_deviation = summarize_dislocations(&executable_positive_values);
    let executable_negative_deviation = summarize_dislocations(&executable_negative_values);
    let factor_gate_passed = (600..=1_440).contains(&executable_overall.observations)
        && effective_events_4h >= 300
        && executable_discovery.observations >= 250
        && executable_validation.observations >= 250
        && executable_positive_deviation.observations >= 100
        && executable_negative_deviation.observations >= 100
        && dislocation_segment_passed(&executable_discovery, &control_discovery)
        && dislocation_segment_passed(&executable_validation, &control_validation)
        && executable_overall
            .mean_forward_1h
            .is_some_and(|value| value > 0.0)
        && executable_overall
            .mean_forward_24h
            .is_some_and(|value| value > 0.0);
    CrossExchangeDislocationPanelReport {
        rule_version: DISLOCATION_RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        okx_symbols,
        binance_audit,
        stages,
        effective_events_4h,
        executable_overall,
        control_overall,
        executable_discovery,
        control_discovery,
        executable_validation,
        control_validation,
        executable_positive_deviation,
        executable_negative_deviation,
        factor_gate_passed,
    }
}

/// 判断单个封存时间段是否覆盖毛收益、命中率和对照增量门槛。
fn segment_passed(
    extreme: &CrossExchangeBasisSummary,
    control: &CrossExchangeBasisSummary,
) -> bool {
    extreme
        .mean_forward_4h
        .zip(control.mean_forward_4h)
        .is_some_and(|(extreme_mean, control_mean)| {
            extreme_mean >= 0.005 && extreme_mean - control_mean >= 0.0025
        })
        && extreme
            .positive_rate_4h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 判断 50bps 首次越界在一个封存时间段的经济幅度和对照增量。
fn dislocation_segment_passed(
    executable: &CrossExchangeBasisSummary,
    control: &CrossExchangeBasisSummary,
) -> bool {
    executable
        .mean_forward_4h
        .zip(control.mean_forward_4h)
        .is_some_and(|(executable_mean, control_mean)| {
            executable_mean >= 0.005 && executable_mean - control_mean >= 0.002
        })
        && executable
            .positive_rate_4h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 汇总一组观察的三个固定期限配对收益与正收益率。
fn summarize(values: &[&BasisObservation]) -> CrossExchangeBasisSummary {
    if values.is_empty() {
        return CrossExchangeBasisSummary::default();
    }
    CrossExchangeBasisSummary {
        observations: values.len(),
        mean_forward_1h: mean(values.iter().map(|value| value.forward_1h)),
        mean_forward_4h: mean(values.iter().map(|value| value.forward_4h)),
        mean_forward_24h: mean(values.iter().map(|value| value.forward_24h)),
        positive_rate_1h_pct: positive_rate(values.iter().map(|value| value.forward_1h)),
        positive_rate_4h_pct: positive_rate(values.iter().map(|value| value.forward_4h)),
        positive_rate_24h_pct: positive_rate(values.iter().map(|value| value.forward_24h)),
    }
}

/// 汇总首次越界观察的三个固定期限配对收益与正收益率。
fn summarize_dislocations(values: &[&DislocationObservation]) -> CrossExchangeBasisSummary {
    if values.is_empty() {
        return CrossExchangeBasisSummary::default();
    }
    CrossExchangeBasisSummary {
        observations: values.len(),
        mean_forward_1h: mean(values.iter().map(|value| value.forward_1h)),
        mean_forward_4h: mean(values.iter().map(|value| value.forward_4h)),
        mean_forward_24h: mean(values.iter().map(|value| value.forward_24h)),
        positive_rate_1h_pct: positive_rate(values.iter().map(|value| value.forward_1h)),
        positive_rate_4h_pct: positive_rate(values.iter().map(|value| value.forward_4h)),
        positive_rate_24h_pct: positive_rate(values.iter().map(|value| value.forward_24h)),
    }
}

/// 将 4 小时内的全市场首次越界归并为一个有效事件。
fn effective_event_count_4h(values: &[&DislocationObservation]) -> usize {
    let mut count = 0usize;
    let mut latest = None::<i64>;
    for value in values {
        if latest.is_none_or(|point| value.decision_ts - point > MS_4H) {
            count += 1;
        }
        latest = Some(value.decision_ts);
    }
    count
}

/// 返回有限非空样本的算术平均值。
fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// 返回有限非空样本大于零的比例，单位百分比。
fn positive_rate(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(values.iter().filter(|value| **value > 0.0).count() as f64 / values.len() as f64 * 100.0)
}

/// 输出可机器读取的数据审计、候选漏斗和全部预注册分组。
pub(super) fn print_report(report: &CrossExchangeBasisPanelReport) {
    println!(
        "cross_exchange_basis_panel\trule={}\tuniverse={}\tokx_symbols={}\tmapped_symbols={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tparsed_rows={}\tdecision_points={}\tcoverage_blocked={}\tfactor_observations={}\tselected={}\textreme={}\tcontrol={}\tincomplete={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.okx_symbols,
        report.binance_audit.mapped_symbols,
        report.binance_audit.mapping_blocked_symbols,
        report.binance_audit.requested_files,
        report.binance_audit.available_files,
        report.binance_audit.missing_files,
        report.binance_audit.invalid_files,
        report.binance_audit.parsed_rows,
        report.stages.decision_points,
        report.stages.coverage_blocked,
        report.stages.factor_observations,
        report.stages.selected_candidates,
        report.stages.extreme_candidates,
        report.stages.control_candidates,
        report.stages.incomplete_outcomes,
        report.factor_gate_passed,
    );
    for (label, value) in [
        ("extreme_overall", &report.extreme_overall),
        ("control_overall", &report.control_overall),
        ("extreme_discovery", &report.extreme_discovery),
        ("control_discovery", &report.control_discovery),
        ("extreme_validation", &report.extreme_validation),
        ("control_validation", &report.control_validation),
        ("extreme_positive_z", &report.extreme_positive_z),
        ("extreme_negative_z", &report.extreme_negative_z),
    ] {
        print_summary(label, value);
    }
}

/// 输出首次越界面板的数据审计、事件漏斗和全部预注册分组。
pub(super) fn print_dislocation_report(report: &CrossExchangeDislocationPanelReport) {
    println!(
        "cross_exchange_dislocation_panel\trule={}\tuniverse={}\tokx_symbols={}\tmapped_symbols={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tparsed_rows={}\tdecision_points={}\tcoverage_blocked={}\tfactor_observations={}\texecutable_crossings={}\tcontrol_crossings={}\tselected={}\tselected_executable={}\tselected_control={}\tincomplete={}\teffective_events_4h={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.okx_symbols,
        report.binance_audit.mapped_symbols,
        report.binance_audit.mapping_blocked_symbols,
        report.binance_audit.requested_files,
        report.binance_audit.available_files,
        report.binance_audit.missing_files,
        report.binance_audit.invalid_files,
        report.binance_audit.parsed_rows,
        report.stages.decision_points,
        report.stages.coverage_blocked,
        report.stages.factor_observations,
        report.stages.executable_crossings,
        report.stages.control_crossings,
        report.stages.selected_candidates,
        report.stages.selected_executable,
        report.stages.selected_control,
        report.stages.incomplete_outcomes,
        report.effective_events_4h,
        report.factor_gate_passed,
    );
    for (label, value) in [
        ("executable_overall", &report.executable_overall),
        ("control_overall", &report.control_overall),
        ("executable_discovery", &report.executable_discovery),
        ("control_discovery", &report.control_discovery),
        ("executable_validation", &report.executable_validation),
        ("control_validation", &report.control_validation),
        (
            "executable_positive_deviation",
            &report.executable_positive_deviation,
        ),
        (
            "executable_negative_deviation",
            &report.executable_negative_deviation,
        ),
    ] {
        print_summary(label, value);
    }
}

/// 输出单个分组的三个固定期限均值和正收益率。
fn print_summary(label: &str, value: &CrossExchangeBasisSummary) {
    println!(
        "cross_exchange_basis_summary\tgroup={}\tobservations={}\tmean_1h={}\tmean_4h={}\tmean_24h={}\tpositive_1h_pct={}\tpositive_4h_pct={}\tpositive_24h_pct={}",
        label,
        value.observations,
        optional(value.mean_forward_1h),
        optional(value.mean_forward_4h),
        optional(value.mean_forward_24h),
        optional(value.positive_rate_1h_pct),
        optional(value.positive_rate_4h_pct),
        optional(value.positive_rate_24h_pct),
    );
}

/// 将缺失浮点指标稳定格式化为 `NA`。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}
