//! 已验证候选账本共用的下一根开盘与冻结退出回放。

use super::*;

/// L1 已完成身份、行情和候选逐字段校验后的回放输入。
pub(in super::super) struct ReplaySource {
    /// 调用方独立的 L2 JSON schema 身份。
    schema_version: &'static str,
    /// 候选、成交、风险、成本与运行隔离身份。
    identity: V10L2Identity,
    /// 已校验的冻结 L1 文件 SHA-256。
    source_l1_report_sha256: String,
    /// 重载行情必须匹配的成员与 K 线指纹。
    dataset_fingerprint_sha256: String,
    /// 本地加载器返回的现时 Top60 成员数量。
    returned_symbol_count: usize,
    /// 具备完整预热和评价窗口的成员数量。
    eligible_symbol_count: usize,
    /// 因缺 K 或指标预热不足而跳过的成员数量。
    excluded_symbol_count: usize,
    /// 同一长期资格 setup 是否只允许第一笔真实成交。
    setup_entry_policy: SetupEntryPolicy,
    /// 初始止损使用固定百分比还是信号时结构价格。
    initial_risk_policy: InitialRiskPolicy,
    /// 目标使用冻结毛 R，还是按同一成本模型反解净 R。
    target_risk_policy: TargetRiskPolicy,
    /// 下一根开盘与止损价已知后执行的成交前风险门禁。
    entry_risk_gate_policy: EntryRiskGatePolicy,
    /// 已与冻结 L1 JSON 逐字段核对的候选账本。
    candidates: Vec<V2Candidate>,
}

impl ReplaySource {
    /// 只允许调用方在完成 L1 文件与重建账本校验后构造回放输入。
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn new(
        schema_version: &'static str,
        identity: V10L2Identity,
        source_l1_report_sha256: String,
        dataset_fingerprint_sha256: String,
        returned_symbol_count: usize,
        eligible_symbol_count: usize,
        excluded_symbol_count: usize,
        setup_entry_policy: SetupEntryPolicy,
        initial_risk_policy: InitialRiskPolicy,
        target_risk_policy: TargetRiskPolicy,
        entry_risk_gate_policy: EntryRiskGatePolicy,
        candidates: Vec<V2Candidate>,
    ) -> Self {
        Self {
            schema_version,
            identity,
            source_l1_report_sha256,
            dataset_fingerprint_sha256,
            returned_symbol_count,
            eligible_symbol_count,
            excluded_symbol_count,
            setup_entry_policy,
            initial_risk_policy,
            target_risk_policy,
            entry_risk_gate_policy,
            candidates,
        }
    }
}

/// 对已验证候选执行与 V10 完全相同的成交、同币种锁、成本和退出合同。
pub(in super::super) fn replay_verified_candidate_ledger(
    data: &BacktestDataSet,
    source: ReplaySource,
) -> V10L2Report {
    let mut blockers = BTreeMap::new();
    let l1_candidates = source.candidates.len();
    let mut entries = Vec::with_capacity(l1_candidates);
    for candidate in source.candidates {
        match resolve_entry(
            data,
            candidate,
            source.initial_risk_policy,
            source.target_risk_policy,
            source.entry_risk_gate_policy,
        ) {
            Ok(entry) => entries.push(entry),
            Err(reason) => *blockers.entry(reason.to_owned()).or_default() += 1,
        }
    }
    let resolved_candidates = entries.len();
    let mut trades =
        simulate_with_symbol_lock(data, entries, &mut blockers, source.setup_entry_policy);
    trades.sort_by(|left, right| {
        (left.signal_ts_ms, left.symbol.as_str(), left.direction).cmp(&(
            right.signal_ts_ms,
            right.symbol.as_str(),
            right.direction,
        ))
    });
    assign_event_clusters(&mut trades);
    for trade in trades.iter().filter(|trade| !trade.complete) {
        *blockers.entry(trade.exit_reason.to_owned()).or_default() += 1;
    }
    let completed = trades
        .iter()
        .filter(|trade| trade.complete)
        .collect::<Vec<_>>();
    let gross = performance(completed.iter().map(|trade| trade.gross_r));
    let net = performance(completed.iter().map(|trade| trade.net_r));
    let net_by_direction = performance_by_direction(&completed);
    let net_by_asset_group = performance_by_asset_group(&completed);
    let concentration = concentration(&completed);
    let contract_identity_verified = trades.iter().all(|trade| {
        contract_is_consistent(
            data,
            trade,
            source.initial_risk_policy,
            source.target_risk_policy,
            source.entry_risk_gate_policy,
        )
    }) && symbol_lock_is_consistent(&trades)
        && setup_entry_policy_is_consistent(&trades, source.setup_entry_policy);
    let coverage = coverage(
        l1_candidates,
        resolved_candidates,
        &trades,
        &completed,
        source.returned_symbol_count,
        source.eligible_symbol_count,
        source.excluded_symbol_count,
        blockers,
    );
    let decision = decide_l2(
        &coverage,
        &gross,
        &net,
        &net_by_direction,
        &concentration,
        true,
        contract_identity_verified,
    );

    V10L2Report {
        schema_version: source.schema_version,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: source.identity,
        source_l1_report_sha256: source.source_l1_report_sha256,
        dataset_fingerprint_sha256: source.dataset_fingerprint_sha256,
        source_candidate_ledger_verified: true,
        coverage,
        gross,
        net,
        net_by_direction,
        net_by_asset_group,
        concentration,
        contract_identity_verified,
        decision,
        trades,
    }
}
