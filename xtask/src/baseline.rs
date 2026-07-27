//! 读取 legacy-allowlist.toml 冻结基线。

use serde::Deserialize;
use std::path::Path;

/// legacy-allowlist.toml 的最小反序列化视图(只取 ratchet 需要的字段)。
#[derive(Debug, Clone, Deserialize)]
pub struct Baseline {
    pub architecture_baseline_git_sha: String,
    #[serde(default)]
    pub dependency_violation: Vec<DependencyViolation>,
    #[serde(default)]
    pub legacy_path: Vec<LegacyPath>,
    pub baseline_counts: BaselineCounts,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DependencyViolation {
    pub id: String,
    pub edge: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegacyPath {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub path_also: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaselineCounts {
    pub dependency_violations: usize,
    pub legacy_paths: usize,
    pub cross_db_direct: usize,
}

impl Baseline {
    /// 从 workspace 根相对路径加载。
    pub fn load(workspace_root: &Path) -> Result<Self, String> {
        let path = workspace_root
            .join("docs/architecture/migrations/baseline-2026-07/legacy-allowlist.toml");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("读取基线失败 {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("解析基线 TOML 失败: {e}"))
    }
}
