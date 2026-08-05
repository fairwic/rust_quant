use anyhow::{bail, Context, Result};
use chrono::Utc;
use rust_quant_cli::app::tradingview_velocity_parity::strict_static_universe::{
    canonical_manifest_sha256, formal_gate, StrictStaticFormalGatePassV2,
    StrictStaticUniverseManifestV2, STRICT_STATIC_CANDLE_INTERVAL_MS,
    STRICT_STATIC_CANDLE_SOURCE_KIND, STRICT_STATIC_MEMBER_COUNT, STRICT_STATIC_VOLUME_FIELD,
};
use rust_quant_cli::app::tradingview_velocity_parity::strict_static_universe_io::{
    decode_and_validate_sealed_snapshot, reaudit_sealed_snapshot_from_quant_core,
    StrictStaticSnapshotV2,
};
use rust_quant_cli::app::tradingview_velocity_parity::{
    aggregate_metric_snapshot, assert_cost_path_parity, concentration_audit, event_cluster_audit,
    replay, symbol_snapshot, verify_v10_pine_source, verify_v11_pine_source,
    verify_v12_pine_source, verify_v13_pine_source, verify_v14_pine_source, verify_v15_pine_source,
    verify_v16_pine_source, verify_v17_pine_source, verify_v18_pine_source, verify_v19_pine_source,
    verify_v20_pine_source, verify_v2_pine_source, verify_v3_pine_source, verify_v4_pine_source,
    verify_v5_pine_source, verify_v6_pine_source, verify_v7_pine_source, verify_v8_pine_source,
    verify_v9_pine_source, ConcentrationAudit, EventClusterAudit, ParityRuleVersion, ReplayConfig,
    ReplayReport, SerializableAggregateSnapshot, SerializableSymbolSnapshot, SignalFamily, Trade,
    STRICT_STATIC_FEE_BPS_PER_SIDE, STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 正式 runner 只接受 sealed 快照、输出目录和显式研究规则版本；
/// 评价窗口与成本口径仍由冻结快照固定，禁止运行时改写。
#[derive(Debug, PartialEq, Eq)]
struct Args {
    /// 已通过冻结流程保存的 sealed 60/60 快照。
    sealed_snapshot: PathBuf,
    /// 正式 JSON 报告的目标目录；文件名由冻结身份自动生成。
    output_dir: PathBuf,
    /// 默认固定 V2；V3～V9 候选必须显式选择，避免历史报告静默换规则。
    rule_version: ParityRuleVersion,
}

/// 报告绑定的静态 cohort 与 canonical manifest 身份。
#[derive(Debug, Serialize)]
struct UniverseIdentity {
    /// 明示这是当前存续合约的静态币池，而不是历史 point-in-time universe。
    cohort_kind: String,
    /// 冻结成员集合、窗口和来源共同定义的版本。
    universe_version: String,
    /// strict static manifest 数据合同版本。
    manifest_schema_version: u32,
    /// 完整 manifest 的 canonical SHA-256。
    manifest_sha256: String,
    /// manifest 生成时间，Unix 毫秒。
    generated_at_ms: i64,
    /// `true` 表示本报告明确接受 current-live 选择带来的幸存者偏差。
    survivorship_bias_accepted: bool,
    /// `true` 表示退市合约没有进入本次静态成员集合。
    delisted_symbols_excluded: bool,
}

/// 选择规则与当次 OKX instrument 快照身份。
#[derive(Debug, Serialize)]
struct SelectionIdentity {
    /// current-live 候选和排名的冻结时刻，Unix 毫秒。
    selection_timestamp_ms: i64,
    /// 完整 instrument 响应实际观测完成时间，Unix 毫秒。
    instrument_snapshot_observed_at_ms: i64,
    /// 选择资格唯一使用的 OKX 公共 instrument 端点。
    instrument_source_endpoint: String,
    /// 当次完整 instrument API envelope 的 SHA-256。
    instrument_snapshot_sha256: String,
    /// 查看覆盖率和回测结果前冻结的选币规则身份。
    selection_rule_id: String,
    /// 固定 60 个成员的可审计选币规则说明。
    selection_rule: String,
}

/// formal gate 通过后写入报告的覆盖摘要。
#[derive(Debug, Serialize)]
struct FormalGateSummary {
    /// 已验证且与 coverage 一致的 canonical manifest SHA-256。
    manifest_sha256: String,
    /// 完整通过门禁的成员数，正式报告固定为 60。
    symbol_count: usize,
    /// 每个成员的 60 天预热 K 线数。
    warmup_candles_per_symbol: usize,
    /// 每个成员正式评价窗口的 K 线数。
    evaluation_candles_per_symbol: usize,
    /// 60 个成员预热加评价窗口的总 K 线数。
    covered_candle_count: usize,
}

/// 本次回放消费的行情语义与严格时间边界。
#[derive(Debug, Serialize)]
struct MarketDataIdentity {
    /// 行情事实来源交易所。
    exchange: String,
    /// 产品类型，严格合同固定为 USDT 永续。
    market_type: String,
    /// 报价币。
    quote_currency: String,
    /// 回放周期。
    timeframe: String,
    /// K 线来源语义，固定为 OKX 已确认 15m `vol_ccy`。
    candle_source_kind: &'static str,
    /// 策略成交量字段；明确不是 `volume * close`。
    volume_field: &'static str,
    /// 预热窗口首根 K 线开盘时间，Unix 毫秒，包含。
    warmup_start_ms: i64,
    /// 正式评价首根 K 线开盘时间，Unix 毫秒，包含。
    evaluation_start_ms: i64,
    /// 正式评价终点，Unix 毫秒，不包含。
    evaluation_end_exclusive_ms: i64,
    /// 交给回放器的末根正式 K 线开盘时间，Unix 毫秒。
    replay_end_ms: i64,
    /// 指标预热天数。
    warmup_days: u32,
}

/// 当前 TradingView Pine 研究实现的不可混淆身份。
#[derive(Debug, Serialize)]
struct StrategyIdentity {
    /// Rust Research 回放规则版本。
    strategy_version: &'static str,
    /// 当前 Pine 源码的冻结 FNV-1a 身份。
    pine_source_fnv1a32: &'static str,
}

/// 一个成本口径下的独立单币汇总和集中度审计。
#[derive(Debug, Serialize)]
struct CostModeReport {
    /// 单边手续费，基点。
    fee_bps_per_side: f64,
    /// 单边滑点，基点。
    slippage_bps_per_side: f64,
    /// 60 个独立固定 1 单位回放的汇总，不代表共享资金组合。
    aggregate: SerializableAggregateSnapshot,
    /// Top1/Top5 交易和盈利币种收益集中度。
    concentration: ConcentrationAudit,
    /// 多空分离的 60 分钟同向 single-linkage 市场事件簇。
    event_clusters_60m: EventClusterAudit,
}

/// 单成员的双成本摘要与零成本逐笔证据。
#[derive(Debug, Serialize)]
struct PerSymbolReport {
    /// manifest 中的 1-based 固定排名。
    rank: u32,
    /// OKX 原始 USDT 永续 symbol。
    symbol: String,
    /// manifest 冻结的 OKX `tickSz` 原始字符串，禁止经 `f64` 往返后写回。
    frozen_tick_size: String,
    /// 零成本单币指标和正式窗口 blocked 数量。
    zero_cost: SerializableSymbolSnapshot,
    /// 5 bps 手续费加 3 bps 滑点单边压力口径摘要。
    stress_cost: SerializableSymbolSnapshot,
    /// 正式评价闭区间内的 blocked signal 数量。
    formal_window_blocked_signal_count: usize,
    /// `true` 表示评价末仍持仓；正式报告不强制结算。
    open_position_at_end: bool,
    /// `true` 表示末根确认信号尚无下一根开盘可成交。
    pending_entry_at_end: bool,
    /// 零成本执行路径的全部已平仓交易明细。
    zero_cost_trades: Vec<Trade>,
}

/// 严格 60/60 runner 的正式 Research 报告。
#[derive(Debug, Serialize)]
struct StrictTop60Report {
    /// 报告结构版本；改变字段语义时必须升级。
    report_schema_version: u32,
    /// 明示只有完整 strict 60/60 才能生成该报告。
    report_scope: &'static str,
    /// 报告生成时间，Unix 毫秒。
    generated_at_ms: i64,
    /// 静态 cohort 与 canonical manifest 身份。
    universe: UniverseIdentity,
    /// current-live 选择和 instrument 响应身份。
    selection: SelectionIdentity,
    /// 正式 60/60 数据门禁摘要。
    formal_gate: FormalGateSummary,
    /// 回放行情来源、字段与窗口身份。
    market_data: MarketDataIdentity,
    /// 当前 Pine/Rust 研究规则身份。
    strategy: StrategyIdentity,
    /// manifest 要求的成员数。
    expected_symbols: usize,
    /// 实际完成零成本和压力成本双回放的成员数。
    included_symbols: usize,
    /// `true` 仅在 60/60 全部完成后生成。
    full_universe_complete: bool,
    /// 零成本结果与稳健性审计。
    zero_cost: CostModeReport,
    /// 固定 5+3 bps 单边压力成本结果与稳健性审计。
    stress_cost: CostModeReport,
    /// 按零成本已平仓交易携带的全部信号家族计数。
    closed_trade_family_counts: BTreeMap<String, usize>,
    /// 严格按 manifest rank 排列的逐币证据。
    per_symbol: Vec<PerSymbolReport>,
    /// 防止把独立单元回放误读成通用组合结论的限制。
    interpretation_limits: Vec<&'static str>,
}

/// 从 sealed 快照只读复审 60/60，并在全部双成本回放通过后原子发布正式报告。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_args(std::env::args().skip(1))?;
    let raw = std::fs::read(&args.sealed_snapshot).with_context(|| {
        format!(
            "读取 strict Top60 sealed 快照失败：{}",
            args.sealed_snapshot.display()
        )
    })?;
    let saved = decode_and_validate_sealed_snapshot(&raw)?;
    let audited = reaudit_sealed_snapshot_from_quant_core(&saved).await?;
    let manifest = audited
        .sealed_manifest
        .as_ref()
        .context("Core 同源复审后缺少 sealed manifest")?;
    let gate = formal_gate(manifest, &audited.coverage)?;
    let manifest_sha256 = canonical_manifest_sha256(manifest)?;

    if gate.manifest_sha256 != manifest_sha256 {
        bail!("formal gate 与 runner 计算的 canonical manifest SHA 不一致");
    }
    verify_selected_pine_source(args.rule_version)?;
    validate_ranked_replay_symbols(&audited, manifest)?;

    let replay_end_ms = last_confirmed_candle_open_ms(manifest.evaluation_end_exclusive_ms)?;
    if replay_end_ms < manifest.evaluation_start_ms {
        bail!("strict Top60 正式评价窗口没有可回放的完整 15m K 线");
    }

    let (zero_reports, stress_reports) =
        replay_all_symbols(&audited, manifest, replay_end_ms, args.rule_version)?;
    let report = build_report(
        &audited,
        manifest,
        gate,
        manifest_sha256.clone(),
        replay_end_ms,
        args.rule_version,
        &zero_reports,
        &stress_reports,
    )?;
    let mut json =
        serde_json::to_vec_pretty(&report).context("序列化 strict Top60 正式报告失败")?;
    json.push(b'\n');
    let file_name = formal_report_file_name(
        &manifest.universe_version,
        &manifest_sha256,
        args.rule_version,
    )?;
    let output = write_atomic_bytes(&args.output_dir, &file_name, &json)?;
    println!("{}", output.display());
    Ok(())
}

/// 解析 sealed 快照、输出目录与可选规则版本；缺省保持 V2 历史行为。
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut sealed_snapshot = None;
    let mut output_dir = None;
    let mut rule_version = ParityRuleVersion::Current3cbbc9d8;
    let mut rule_version_seen = false;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--sealed-snapshot" => {
                if sealed_snapshot.is_some() {
                    bail!("--sealed-snapshot 不能重复");
                }
                sealed_snapshot = Some(PathBuf::from(
                    args.next()
                        .context("--sealed-snapshot requires a file path")?,
                ));
            }
            "--output-dir" => {
                if output_dir.is_some() {
                    bail!("--output-dir 不能重复");
                }
                output_dir = Some(PathBuf::from(
                    args.next().context("--output-dir requires a directory")?,
                ));
            }
            "--rule-version" => {
                if rule_version_seen {
                    bail!("--rule-version 不能重复");
                }
                rule_version_seen = true;
                rule_version = match args
                    .next()
                    .context(
                        "--rule-version requires current-v2 or candidate-v3 through candidate-v20",
                    )?
                    .as_str()
                {
                    "current-v2" => ParityRuleVersion::Current3cbbc9d8,
                    "candidate-v3" => ParityRuleVersion::CandidateV3,
                    "candidate-v4" => ParityRuleVersion::CandidateV4,
                    "candidate-v5" => ParityRuleVersion::CandidateV5,
                    "candidate-v6" => ParityRuleVersion::CandidateV6,
                    "candidate-v7" => ParityRuleVersion::CandidateV7,
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
                    other => bail!("strict Top60 不支持规则版本：{other}"),
                };
            }
            other => bail!("strict Top60 runner 不接受参数：{other}"),
        }
    }
    Ok(Args {
        sealed_snapshot: sealed_snapshot.context("--sealed-snapshot is required")?,
        output_dir: output_dir.context("--output-dir is required")?,
        rule_version,
    })
}

/// 按显式规则身份校验对应 Pine 文件；候选 V3 不允许退回 V2 快照。
fn verify_selected_pine_source(rule_version: ParityRuleVersion) -> Result<()> {
    match rule_version {
        ParityRuleVersion::Current3cbbc9d8 => verify_v2_pine_source(),
        ParityRuleVersion::CandidateV3 => verify_v3_pine_source(),
        ParityRuleVersion::CandidateV4 => verify_v4_pine_source(),
        ParityRuleVersion::CandidateV5 => verify_v5_pine_source(),
        ParityRuleVersion::CandidateV6 => verify_v6_pine_source(),
        ParityRuleVersion::CandidateV7 => verify_v7_pine_source(),
        ParityRuleVersion::CandidateV8 => verify_v8_pine_source(),
        ParityRuleVersion::CandidateV9 => verify_v9_pine_source(),
        ParityRuleVersion::CandidateV10 => verify_v10_pine_source(),
        ParityRuleVersion::CandidateV11 => verify_v11_pine_source(),
        ParityRuleVersion::CandidateV12 => verify_v12_pine_source(),
        ParityRuleVersion::CandidateV13 => verify_v13_pine_source(),
        ParityRuleVersion::CandidateV14 => verify_v14_pine_source(),
        ParityRuleVersion::CandidateV15 => verify_v15_pine_source(),
        ParityRuleVersion::CandidateV16 => verify_v16_pine_source(),
        ParityRuleVersion::CandidateV17 => verify_v17_pine_source(),
        ParityRuleVersion::CandidateV18 => verify_v18_pine_source(),
        ParityRuleVersion::CandidateV19 => verify_v19_pine_source(),
        ParityRuleVersion::CandidateV20 => verify_v20_pine_source(),
        ParityRuleVersion::Frozen66d3937e => {
            bail!("strict Top60 runner 只支持 current-v2 或 candidate-v3 至 candidate-v20")
        }
    }
}

/// 要求运行时回放输入与 manifest 的 rank、symbol 和原始 tick 一一对应。
fn validate_ranked_replay_symbols(
    snapshot: &StrictStaticSnapshotV2,
    manifest: &StrictStaticUniverseManifestV2,
) -> Result<()> {
    if snapshot.symbols.len() != STRICT_STATIC_MEMBER_COUNT
        || manifest.members.len() != STRICT_STATIC_MEMBER_COUNT
    {
        bail!(
            "strict Top60 runner 必须取得恰好 60 个回放输入，snapshot={} manifest={}",
            snapshot.symbols.len(),
            manifest.members.len()
        );
    }
    for (index, (actual, member)) in snapshot.symbols.iter().zip(&manifest.members).enumerate() {
        let expected_rank = u32::try_from(index + 1).context("strict Top60 rank 溢出")?;
        if member.rank != expected_rank || actual.symbol != member.symbol {
            bail!(
                "strict Top60 第 {} 名回放输入与 manifest 不一致：actual={} manifest={}",
                expected_rank,
                actual.symbol,
                member.symbol
            );
        }
        let manifest_tick = member
            .frozen_tick_size
            .parse::<f64>()
            .with_context(|| format!("{} frozen tick 不能用于回放", member.symbol))?;
        if !manifest_tick.is_finite()
            || manifest_tick <= 0.0
            || actual.tick_size.to_bits() != manifest_tick.to_bits()
        {
            bail!("{} 回放 tick 与 manifest 原始 tick 不一致", member.symbol);
        }
    }
    Ok(())
}

/// 对每个完整成员同时运行零成本和冻结压力成本，并逐币验证执行路径没有漂移。
fn replay_all_symbols(
    snapshot: &StrictStaticSnapshotV2,
    manifest: &StrictStaticUniverseManifestV2,
    replay_end_ms: i64,
    rule_version: ParityRuleVersion,
) -> Result<(Vec<ReplayReport>, Vec<ReplayReport>)> {
    let mut zero_reports = Vec::with_capacity(STRICT_STATIC_MEMBER_COUNT);
    let mut stress_reports = Vec::with_capacity(STRICT_STATIC_MEMBER_COUNT);
    for symbol in &snapshot.symbols {
        let zero_config = match rule_version {
            ParityRuleVersion::Current3cbbc9d8 => ReplayConfig::current_pine_v2(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV3 => ReplayConfig::current_pine_v3(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV4 => ReplayConfig::current_pine_v4(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV5 => ReplayConfig::current_pine_v5(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV6 => ReplayConfig::current_pine_v6(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV7 => ReplayConfig::current_pine_v7(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV8 => ReplayConfig::current_pine_v8(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV9 => ReplayConfig::current_pine_v9(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV10 => ReplayConfig::current_pine_v10(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV11 => ReplayConfig::current_pine_v11(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV12 => ReplayConfig::current_pine_v12(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV13 => ReplayConfig::current_pine_v13(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV14 => ReplayConfig::current_pine_v14(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV15 => ReplayConfig::current_pine_v15(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV16 => ReplayConfig::current_pine_v16(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV17 => ReplayConfig::current_pine_v17(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV18 => ReplayConfig::current_pine_v18(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV19 => ReplayConfig::current_pine_v19(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::CandidateV20 => ReplayConfig::current_pine_v20(
                symbol.symbol.clone(),
                symbol.tick_size,
                manifest.evaluation_start_ms,
                replay_end_ms,
            ),
            ParityRuleVersion::Frozen66d3937e => {
                bail!("strict Top60 runner 只支持 current-v2 或 candidate-v3 至 candidate-v20")
            }
        };
        // candles 有意保留完整 60 天预热；ReplayConfig 只允许正式窗口产生入场与指标。
        let zero = replay(&symbol.candles, zero_config.clone());
        let stress = replay(
            &symbol.candles,
            ReplayConfig {
                fee_bps_per_side: STRICT_STATIC_FEE_BPS_PER_SIDE,
                slippage_bps_per_side: STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE,
                ..zero_config
            },
        );
        assert_cost_path_parity(&zero, &stress)
            .with_context(|| format!("{} 双成本执行路径不一致", symbol.symbol))?;
        zero_reports.push(zero);
        stress_reports.push(stress);
    }
    Ok((zero_reports, stress_reports))
}

/// 在全部成员完成后组装一次性报告，避免中途失败留下部分结果。
fn build_report(
    snapshot: &StrictStaticSnapshotV2,
    manifest: &StrictStaticUniverseManifestV2,
    gate: StrictStaticFormalGatePassV2,
    manifest_sha256: String,
    replay_end_ms: i64,
    rule_version: ParityRuleVersion,
    zero_reports: &[ReplayReport],
    stress_reports: &[ReplayReport],
) -> Result<StrictTop60Report> {
    if zero_reports.len() != STRICT_STATIC_MEMBER_COUNT
        || stress_reports.len() != STRICT_STATIC_MEMBER_COUNT
    {
        bail!("strict Top60 双成本回放没有完整达到 60/60");
    }
    let per_symbol = snapshot
        .symbols
        .iter()
        .zip(&manifest.members)
        .zip(zero_reports.iter().zip(stress_reports))
        .map(|((input, member), (zero, stress))| {
            if input.symbol != zero.symbol || zero.symbol != stress.symbol {
                bail!("{} 逐币报告与双成本回放顺序不一致", member.symbol);
            }
            let zero_snapshot = symbol_snapshot(zero, manifest.evaluation_start_ms, replay_end_ms);
            let stress_snapshot =
                symbol_snapshot(stress, manifest.evaluation_start_ms, replay_end_ms);
            Ok(PerSymbolReport {
                rank: member.rank,
                symbol: member.symbol.clone(),
                frozen_tick_size: member.frozen_tick_size.clone(),
                formal_window_blocked_signal_count: zero_snapshot.blocked_signal_count,
                open_position_at_end: zero.open_position_at_end,
                pending_entry_at_end: zero.pending_entry_at_end,
                zero_cost: zero_snapshot,
                stress_cost: stress_snapshot,
                zero_cost_trades: zero.trades.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(StrictTop60Report {
        report_schema_version: 3,
        report_scope: "strict_surviving_static_top60_60_of_60",
        generated_at_ms: Utc::now().timestamp_millis(),
        universe: UniverseIdentity {
            cohort_kind: snapshot.selection_plan.cohort_kind.clone(),
            universe_version: manifest.universe_version.clone(),
            manifest_schema_version: manifest.schema_version,
            manifest_sha256,
            generated_at_ms: manifest.generated_at_ms,
            survivorship_bias_accepted: snapshot.selection_plan.survivorship_bias_accepted,
            delisted_symbols_excluded: snapshot.selection_plan.delisted_symbols_excluded,
        },
        selection: SelectionIdentity {
            selection_timestamp_ms: manifest.selection_timestamp_ms,
            instrument_snapshot_observed_at_ms: manifest.instrument_snapshot_observed_at_ms,
            instrument_source_endpoint: manifest.instrument_source_endpoint.clone(),
            instrument_snapshot_sha256: manifest.instrument_snapshot_sha256.clone(),
            selection_rule_id: manifest.selection_rule_id.clone(),
            selection_rule: manifest.selection_rule.clone(),
        },
        formal_gate: FormalGateSummary {
            manifest_sha256: gate.manifest_sha256,
            symbol_count: gate.symbol_count,
            warmup_candles_per_symbol: gate.warmup_candles_per_symbol,
            evaluation_candles_per_symbol: gate.evaluation_candles_per_symbol,
            covered_candle_count: gate.covered_candle_count,
        },
        market_data: MarketDataIdentity {
            exchange: manifest.exchange.clone(),
            market_type: manifest.market_type.clone(),
            quote_currency: manifest.quote_currency.clone(),
            timeframe: manifest.timeframe.clone(),
            candle_source_kind: STRICT_STATIC_CANDLE_SOURCE_KIND,
            volume_field: STRICT_STATIC_VOLUME_FIELD,
            warmup_start_ms: manifest.warmup_start_ms,
            evaluation_start_ms: manifest.evaluation_start_ms,
            evaluation_end_exclusive_ms: manifest.evaluation_end_exclusive_ms,
            replay_end_ms,
            warmup_days: manifest.warmup_days,
        },
        strategy: StrategyIdentity {
            strategy_version: rule_version.strategy_version(),
            pine_source_fnv1a32: rule_version.pine_source_fnv1a32(),
        },
        expected_symbols: STRICT_STATIC_MEMBER_COUNT,
        included_symbols: zero_reports.len(),
        full_universe_complete: true,
        zero_cost: cost_mode_report(0.0, 0.0, zero_reports),
        stress_cost: cost_mode_report(
            STRICT_STATIC_FEE_BPS_PER_SIDE,
            STRICT_STATIC_SLIPPAGE_BPS_PER_SIDE,
            stress_reports,
        ),
        closed_trade_family_counts: family_counts(zero_reports),
        per_symbol,
        interpretation_limits: vec![
            "fixed 1 unit per symbol; aggregate is not a unified-capital, capacity, leverage, or correlation-constrained portfolio",
            "the cohort is selected from currently live OKX contracts and therefore has survivorship bias; it is not a historical point-in-time universe",
            "symbols not live at selection time are excluded; after freeze, members are never substituted, and later listing-status changes do not silently alter the cohort",
            "cost stress changes only net PnL/net R/metrics; signals, fills, stops, exits, blocked signals, and end states must remain identical",
            "60m same-direction clusters are a deterministic event-concentration audit, not a sector/correlation portfolio risk model",
        ],
    })
}

/// 生成一个成本口径的汇总、头部贡献和 60 分钟事件簇。
fn cost_mode_report(
    fee_bps_per_side: f64,
    slippage_bps_per_side: f64,
    reports: &[ReplayReport],
) -> CostModeReport {
    CostModeReport {
        fee_bps_per_side,
        slippage_bps_per_side,
        aggregate: aggregate_metric_snapshot(reports),
        concentration: concentration_audit(reports),
        event_clusters_60m: event_cluster_audit(reports),
    }
}

/// 按已平仓交易携带的全部家族计数；同一笔多家族交易会分别贡献一次。
fn family_counts(reports: &[ReplayReport]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for family in reports
        .iter()
        .flat_map(|report| &report.trades)
        .flat_map(|trade| trade.families.iter().copied())
    {
        *counts.entry(family_name(family).to_owned()).or_default() += 1;
    }
    counts
}

/// 返回稳定报告键，避免 Rust `Debug` 文本变化破坏研究产物。
fn family_name(family: SignalFamily) -> &'static str {
    match family {
        SignalFamily::RsiBullishDivergence => "rsi_bullish_divergence",
        SignalFamily::RsiBearishDivergence => "rsi_bearish_divergence",
        SignalFamily::RsiOversoldPattern => "rsi_oversold_pattern",
        SignalFamily::RsiOverboughtPattern => "rsi_overbought_pattern",
        SignalFamily::EmaTrendLong => "ema_trend_long",
        SignalFamily::EmaTrendShort => "ema_trend_short",
        SignalFamily::ConfirmedRangeAcceptanceLong => "confirmed_range_acceptance_long",
        SignalFamily::LargeHorizontalRangeBreakLong => "large_horizontal_range_break_long",
        SignalFamily::StrictVisualConsolidationBreakLong => {
            "strict_visual_consolidation_break_long"
        }
        SignalFamily::StrictVisualConsolidationBreakShort => {
            "strict_visual_consolidation_break_short"
        }
        SignalFamily::LargeAscendingTriangleBreakLong => "large_ascending_triangle_break_long",
        SignalFamily::AnchorFalseBreakShort => "anchor_false_break_short",
        SignalFamily::AnchorUpthrustFailedAcceptanceShort => {
            "anchor_upthrust_failed_acceptance_short"
        }
        SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort => {
            "anchor_upthrust_failed_acceptance_right_side_short"
        }
        SignalFamily::TransitionLiquiditySweepShort => "transition_liquidity_sweep_short",
        SignalFamily::EmaCompressionExpansionLong => "ema_compression_expansion_long",
        SignalFamily::EmaCompressionExpansionShort => "ema_compression_expansion_short",
        SignalFamily::ThreeBarBullishEngulfingLong => "three_bar_bullish_engulfing_long",
        SignalFamily::EffortNoResultShort => "effort_no_result_short",
        SignalFamily::BollingerLowerReclaimLong => "bollinger_lower_reclaim_long",
        SignalFamily::Ema596ReclaimDepartureLong => "ema596_reclaim_departure_long",
        SignalFamily::RangeSqueezeBreakAcceptanceLong => "range_squeeze_break_acceptance_long",
        SignalFamily::RangeSqueezeBreakAcceptanceShort => "range_squeeze_break_acceptance_short",
        SignalFamily::RangeSqueezeRightSideTriggerLong => "range_squeeze_right_side_trigger_long",
        SignalFamily::RangeSqueezeRightSideTriggerShort => "range_squeeze_right_side_trigger_short",
        SignalFamily::RangeSqueezeRightSideTriggerAblationLong => {
            "range_squeeze_right_side_trigger_ablation_long"
        }
        SignalFamily::RangeSqueezeRightSideTriggerAblationShort => {
            "range_squeeze_right_side_trigger_ablation_short"
        }
        SignalFamily::SellClimaxBaseReclaimLong => "sell_climax_base_reclaim_long",
    }
}

/// 把半开评价终点转换为回放器使用的末根 15m 开盘时间。
fn last_confirmed_candle_open_ms(evaluation_end_exclusive_ms: i64) -> Result<i64> {
    if evaluation_end_exclusive_ms <= 0
        || evaluation_end_exclusive_ms.rem_euclid(STRICT_STATIC_CANDLE_INTERVAL_MS) != 0
    {
        bail!("strict Top60 evaluation_end_exclusive_ms 必须是正数且对齐 15m");
    }
    evaluation_end_exclusive_ms
        .checked_sub(STRICT_STATIC_CANDLE_INTERVAL_MS)
        .filter(|timestamp_ms| *timestamp_ms >= 0)
        .context("strict Top60 末根正式 K 线时间下溢")
}

/// 生成绑定策略版本、Pine 身份和 manifest SHA 的正式文件名，避免不同规则版本互相覆盖。
fn formal_report_file_name(
    universe_version: &str,
    manifest_sha256: &str,
    rule_version: ParityRuleVersion,
) -> Result<String> {
    if manifest_sha256.len() != 64 || !manifest_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("strict Top60 manifest SHA-256 不是 64 位十六进制");
    }
    let strategy = sanitized_file_token(rule_version.strategy_version(), "strategy");
    let pine_hash = sanitized_file_token(rule_version.pine_source_fnv1a32(), "pine");
    let universe = sanitized_file_token(universe_version, "universe");
    Ok(format!(
        "tradingview_velocity_strict_top60_{}_{}_{}_{}.json",
        strategy,
        pine_hash,
        universe,
        manifest_sha256[..12].to_ascii_lowercase()
    ))
}

/// 把外部版本文本约束为稳定文件名 token；不把路径分隔符带入输出目录。
fn sanitized_file_token(value: &str, fallback: &'static str) -> String {
    let mut sanitized = String::new();
    let mut previous_separator = false;
    for character in value.chars().take(96) {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            sanitized.push(character);
            previous_separator = false;
        } else if !previous_separator && !sanitized.is_empty() {
            sanitized.push('_');
            previous_separator = true;
        }
    }
    let sanitized = sanitized.trim_matches('_');
    if sanitized.is_empty() {
        fallback.to_owned()
    } else {
        sanitized.to_owned()
    }
}

/// 先在目标目录写完并同步临时文件，最后一步原子 rename；失败路径删除临时文件。
fn write_atomic_bytes(output_dir: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("创建 strict Top60 输出目录失败：{}", output_dir.display()))?;
    let final_path = output_dir.join(file_name);
    let temporary_name = format!(
        ".{}.{}-{}.tmp",
        file_name,
        std::process::id(),
        Utc::now().timestamp_micros()
    );
    let temporary_path = output_dir.join(temporary_name);
    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .with_context(|| format!("创建临时报告失败：{}", temporary_path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("写入临时报告失败：{}", temporary_path.display()))?;
        file.sync_all()
            .with_context(|| format!("同步临时报告失败：{}", temporary_path.display()))?;
        std::fs::rename(&temporary_path, &final_path).with_context(|| {
            format!(
                "原子发布 strict Top60 报告失败：{} -> {}",
                temporary_path.display(),
                final_path.display()
            )
        })
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试使用唯一临时目录，避免并行执行互相覆盖。
    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rust_quant_{name}_{}_{}",
            std::process::id(),
            Utc::now().timestamp_micros()
        ))
    }

    #[test]
    fn parser_defaults_to_v2_and_accepts_explicit_candidates() {
        let args = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
        ])
        .expect("strict args");

        assert_eq!(args.sealed_snapshot, PathBuf::from("sealed.json"));
        assert_eq!(args.output_dir, PathBuf::from("reports"));
        assert_eq!(args.rule_version, ParityRuleVersion::Current3cbbc9d8);

        let candidate = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v3".to_owned(),
        ])
        .expect("candidate V3 args");
        assert_eq!(candidate.rule_version, ParityRuleVersion::CandidateV3);

        let candidate_v4 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v4".to_owned(),
        ])
        .expect("candidate V4 args");
        assert_eq!(candidate_v4.rule_version, ParityRuleVersion::CandidateV4);

        let candidate_v5 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v5".to_owned(),
        ])
        .expect("candidate V5 args");
        assert_eq!(candidate_v5.rule_version, ParityRuleVersion::CandidateV5);

        let candidate_v6 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v6".to_owned(),
        ])
        .expect("candidate V6 args");
        assert_eq!(candidate_v6.rule_version, ParityRuleVersion::CandidateV6);

        let candidate_v7 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v7".to_owned(),
        ])
        .expect("candidate V7 args");
        assert_eq!(candidate_v7.rule_version, ParityRuleVersion::CandidateV7);

        let candidate_v8 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v8".to_owned(),
        ])
        .expect("candidate V8 args");
        assert_eq!(candidate_v8.rule_version, ParityRuleVersion::CandidateV8);

        let candidate_v9 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v9".to_owned(),
        ])
        .expect("candidate V9 args");
        assert_eq!(candidate_v9.rule_version, ParityRuleVersion::CandidateV9);

        let candidate_v10 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v10".to_owned(),
        ])
        .expect("candidate V10 args");
        assert_eq!(candidate_v10.rule_version, ParityRuleVersion::CandidateV10);

        let candidate_v11 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v11".to_owned(),
        ])
        .expect("candidate V11 args");
        assert_eq!(candidate_v11.rule_version, ParityRuleVersion::CandidateV11);

        let candidate_v12 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v12".to_owned(),
        ])
        .expect("candidate V12 args");
        assert_eq!(candidate_v12.rule_version, ParityRuleVersion::CandidateV12);

        let candidate_v13 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v13".to_owned(),
        ])
        .expect("candidate V13 args");
        assert_eq!(candidate_v13.rule_version, ParityRuleVersion::CandidateV13);

        let candidate_v14 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v14".to_owned(),
        ])
        .expect("candidate V14 args");
        assert_eq!(candidate_v14.rule_version, ParityRuleVersion::CandidateV14);

        let candidate_v15 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v15".to_owned(),
        ])
        .expect("candidate V15 args");
        assert_eq!(candidate_v15.rule_version, ParityRuleVersion::CandidateV15);

        let candidate_v16 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v16".to_owned(),
        ])
        .expect("candidate V16 args");
        assert_eq!(candidate_v16.rule_version, ParityRuleVersion::CandidateV16);

        let candidate_v17 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v17".to_owned(),
        ])
        .expect("candidate V17 args");
        assert_eq!(candidate_v17.rule_version, ParityRuleVersion::CandidateV17);

        let candidate_v18 = parse_args([
            "--sealed-snapshot".to_owned(),
            "sealed.json".to_owned(),
            "--output-dir".to_owned(),
            "reports".to_owned(),
            "--rule-version".to_owned(),
            "candidate-v18".to_owned(),
        ])
        .expect("candidate V18 args");
        assert_eq!(candidate_v18.rule_version, ParityRuleVersion::CandidateV18);

        assert!(parse_args(["--output".to_owned(), "x".to_owned()]).is_err());
        assert!(parse_args(["--sealed-snapshot".to_owned(), "x".to_owned()]).is_err());
        assert!(parse_args([
            "--sealed-snapshot".to_owned(),
            "x".to_owned(),
            "--output-dir".to_owned(),
            "out".to_owned(),
            "--rule-version".to_owned(),
            "frozen-v1".to_owned(),
        ])
        .is_err());
        assert!(parse_args([
            "--sealed-snapshot".to_owned(),
            "a".to_owned(),
            "--sealed-snapshot".to_owned(),
            "b".to_owned(),
            "--output-dir".to_owned(),
            "out".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn formal_file_name_sanitizes_version_and_binds_sha_prefix() {
        let sha = "A1".repeat(32);
        let file_name =
            formal_report_file_name("Top 60/2026 中文", &sha, ParityRuleVersion::Current3cbbc9d8)
                .expect("safe formal file name");

        assert_eq!(
            file_name,
            "tradingview_velocity_strict_top60_tradingview_velocity_parity_15m_research_v2_3cbbc9d8_Top_60_2026_a1a1a1a1a1a1.json"
        );
        assert!(
            formal_report_file_name("valid", "abc", ParityRuleVersion::Current3cbbc9d8).is_err()
        );
    }

    #[test]
    fn atomic_writer_publishes_only_the_final_complete_file() {
        let directory = test_directory("strict_top60_atomic");
        let output = write_atomic_bytes(&directory, "formal.json", b"{\"complete\":true}\n")
            .expect("atomic report");
        let entries = std::fs::read_dir(&directory)
            .expect("read output directory")
            .map(|entry| entry.expect("directory entry").file_name())
            .collect::<Vec<_>>();

        assert_eq!(output, directory.join("formal.json"));
        assert_eq!(
            std::fs::read(&output).expect("read formal report"),
            b"{\"complete\":true}\n"
        );
        assert_eq!(entries, vec![std::ffi::OsString::from("formal.json")]);

        std::fs::remove_file(output).expect("remove test report");
        std::fs::remove_dir(directory).expect("remove test directory");
    }

    #[test]
    fn replay_end_is_the_last_completed_fifteen_minute_open() {
        assert_eq!(
            last_confirmed_candle_open_ms(2 * STRICT_STATIC_CANDLE_INTERVAL_MS)
                .expect("aligned end"),
            STRICT_STATIC_CANDLE_INTERVAL_MS
        );
        assert!(last_confirmed_candle_open_ms(STRICT_STATIC_CANDLE_INTERVAL_MS - 1).is_err());
        assert!(last_confirmed_candle_open_ms(0).is_err());
    }
}
