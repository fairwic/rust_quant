use super::holm_bonferroni_adjust;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 生成研究证据时使用的 Git 与目标源码身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    /// 仓库当前 HEAD；dirty 状态由独立字段表达。
    pub git_head: String,
    /// 目标研究源码按稳定路径顺序计算的 SHA-256 指纹。
    pub source_fingerprint: String,
    /// 目标研究源码是否含未提交变化。
    pub dirty: bool,
}

/// 实验在账本中的生命周期状态，不映射到生产 Shadow/Paper/Live。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    /// 方案已经预注册，但尚未消费数据。
    Preregistered,
    /// 预注册实验已经执行并形成结论。
    Completed,
    /// 研究方向已冻结归档。
    ArchivedResearch,
    /// 仅保留为未来新数据验证假设。
    RetainedForFutureValidation,
}

/// 单个实验及其数据、源码、统计结论的不可变审计记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentLedgerEntry {
    /// 本批次内唯一实验 ID。
    pub experiment_id: String,
    /// 派生实验的父 ID；首个实验为空。
    pub parent_experiment_id: Option<String>,
    /// 数据打开前冻结的可证伪假设。
    pub hypothesis: String,
    /// 固定的评估协议版本。
    pub protocol_version: String,
    /// 不可变策略键。
    pub strategy_key: String,
    /// 单一 K 线周期。
    pub timeframe: String,
    /// 本实验覆盖的市场白名单。
    pub markets: Vec<String>,
    /// 是否在读取结果前完成预注册。
    pub preregistered: bool,
    /// true 表示不能被解释为可晋级生产证据。
    pub research_only: bool,
    /// 本次输入数据的稳定指纹。
    pub dataset_fingerprint: String,
    /// 本次运行绑定的目标源码身份。
    pub source_identity: SourceIdentity,
    /// 实验生命周期状态。
    pub status: ExperimentStatus,
    /// 预注册单侧检验的原始 p 值；没有可靠检验时保持为空。
    pub raw_one_sided_p: Option<f64>,
    /// 同一批次 Holm-Bonferroni 调整值，由账本统一写入。
    pub holm_adjusted_p: Option<f64>,
    /// 固定门禁得出的结论。
    pub decision: Option<String>,
    /// 归档或继续的明确原因。
    pub reason: Option<String>,
}

/// 同一多重检验家族的实验账本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentLedger {
    /// 按预注册顺序保存的实验记录。
    pub entries: Vec<ExperimentLedgerEntry>,
}

impl ExperimentLedger {
    /// 校验账本身份和预注册约束，并只为存在原始 p 值的条目写入 Holm 调整值。
    pub fn validate_and_adjust_holm(&mut self) -> Result<(), String> {
        let mut ids = HashSet::with_capacity(self.entries.len());
        for entry in &self.entries {
            if entry.experiment_id.trim().is_empty()
                || !ids.insert(entry.experiment_id.as_str())
                || entry.hypothesis.trim().is_empty()
                || entry.protocol_version.trim().is_empty()
                || entry.strategy_key.trim().is_empty()
                || entry.timeframe.trim().is_empty()
                || entry.markets.is_empty()
                || entry.markets.iter().any(|market| market.trim().is_empty())
                || entry.dataset_fingerprint.trim().is_empty()
                || entry.source_identity.git_head.trim().is_empty()
                || entry.source_identity.source_fingerprint.trim().is_empty()
            {
                return Err(
                    "experiment ledger contains duplicate or incomplete identity".to_owned(),
                );
            }
            if !entry.research_only && !entry.preregistered {
                return Err("promotable experiments must be preregistered".to_owned());
            }
            if entry
                .raw_one_sided_p
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            {
                return Err("raw one-sided p-values must be between zero and one".to_owned());
            }
        }

        let indexed_p_values = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.raw_one_sided_p.map(|value| (index, value)))
            .collect::<Vec<_>>();
        let adjusted = holm_bonferroni_adjust(
            &indexed_p_values
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>(),
        )?;
        for entry in &mut self.entries {
            entry.holm_adjusted_p = None;
        }
        for ((entry_index, _), adjusted_p) in indexed_p_values.into_iter().zip(adjusted) {
            self.entries[entry_index].holm_adjusted_p = Some(adjusted_p);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_entry(id: &str, raw_p: Option<f64>) -> ExperimentLedgerEntry {
        ExperimentLedgerEntry {
            experiment_id: id.to_owned(),
            parent_experiment_id: None,
            hypothesis: "confirmation retains positive expectancy after delay".to_owned(),
            protocol_version: "pa-diagnostic-v7-abc-counterfactual".to_owned(),
            strategy_key: "pa_trend_15m".to_owned(),
            timeframe: "15m".to_owned(),
            markets: vec!["BTC-USDT-SWAP".to_owned()],
            preregistered: true,
            research_only: true,
            dataset_fingerprint: "sha256:dataset".to_owned(),
            source_identity: SourceIdentity {
                git_head: "0123456789abcdef".to_owned(),
                source_fingerprint: "sha256:source".to_owned(),
                dirty: false,
            },
            status: ExperimentStatus::Completed,
            raw_one_sided_p: raw_p,
            holm_adjusted_p: None,
            decision: Some("archive_pa_standalone".to_owned()),
            reason: Some("pre-registered gate failed".to_owned()),
        }
    }

    #[test]
    fn rejects_duplicate_ids_and_missing_source_fingerprint() {
        let entry = valid_entry("duplicate", None);
        let mut duplicate = ExperimentLedger {
            entries: vec![entry.clone(), entry],
        };
        assert!(duplicate.validate_and_adjust_holm().is_err());

        let mut missing_source = ExperimentLedger {
            entries: vec![valid_entry("missing-source", None)],
        };
        missing_source.entries[0]
            .source_identity
            .source_fingerprint
            .clear();
        assert!(missing_source.validate_and_adjust_holm().is_err());
    }

    #[test]
    fn rejects_non_preregistered_promotable_entries_and_invalid_p_values() {
        let mut promotable = ExperimentLedger {
            entries: vec![valid_entry("promotable", None)],
        };
        promotable.entries[0].research_only = false;
        promotable.entries[0].preregistered = false;
        assert!(promotable.validate_and_adjust_holm().is_err());

        let mut invalid_p = ExperimentLedger {
            entries: vec![valid_entry("invalid-p", Some(1.1))],
        };
        assert!(invalid_p.validate_and_adjust_holm().is_err());
    }

    #[test]
    fn writes_holm_adjustments_in_original_entry_order() {
        let mut ledger = ExperimentLedger {
            entries: vec![
                valid_entry("first", Some(0.04)),
                valid_entry("no-p", None),
                valid_entry("third", Some(0.01)),
                valid_entry("fourth", Some(0.03)),
            ],
        };

        ledger.validate_and_adjust_holm().unwrap();

        assert_eq!(ledger.entries[0].holm_adjusted_p, Some(0.06));
        assert_eq!(ledger.entries[1].holm_adjusted_p, None);
        assert_eq!(ledger.entries[2].holm_adjusted_p, Some(0.03));
        assert_eq!(ledger.entries[3].holm_adjusted_p, Some(0.06));
    }
}
