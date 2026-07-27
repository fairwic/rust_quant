//! arch-check 的结果数据模型。

use serde::Serialize;

/// 单条违规发现。
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    /// 规则类别 slug(dependency / cross-db / file-size / ...)
    pub category: String,
    /// 人读描述
    pub detail: String,
    /// 相关位置(crate 名、边、文件路径)
    pub location: String,
    /// 若匹配到已登记 legacy 基线,填其 id(V1..);新增违规为 None
    pub baseline_id: Option<String>,
}

/// 单个检查器的结果。
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: String,
    /// 本检查器发现的全部违规(含基线内的)
    pub violations: Vec<Violation>,
    /// 仅告警、不计入 ratchet 的项(如行数 WARN)
    pub warnings: Vec<String>,
}

impl CheckResult {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            violations: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// arch-check 整体结果 + ratchet 判定。
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub checks: Vec<CheckResult>,
    /// 新增(基线外)违规:导致 FAIL
    pub new_violations: Vec<Violation>,
    /// 已消除的基线违规(违规数下降,好事,仅提示)
    pub resolved_baseline_ids: Vec<String>,
    /// 硬失败:文件超过 2000 行硬上限
    pub hard_size_failures: Vec<String>,
}

impl Outcome {
    pub fn failed(&self) -> bool {
        !self.new_violations.is_empty() || !self.hard_size_failures.is_empty()
    }
}
