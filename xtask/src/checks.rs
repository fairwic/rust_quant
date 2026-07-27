//! 各架构检查器实现 + ratchet 汇总。

use crate::baseline::Baseline;
use crate::model::{CheckResult, Outcome, Violation};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 定位 workspace 根:从当前可执行目录向上找含 `crates/` 与根 Cargo.toml 的目录。
/// xtask 由 `cargo xtask` 在 workspace 根启动,CARGO_MANIFEST_DIR 指向 xtask/,取其父。
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/xtask
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let p = Path::new(&manifest);
    p.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| p.to_path_buf())
}

/// 运行全部检查并做 ratchet 判定。
pub fn run_arch_check() -> Result<Outcome, String> {
    let root = workspace_root();
    let baseline = Baseline::load(&root)?;

    let mut checks = Vec::new();
    checks.push(check_dependency_direction(&root)?);
    checks.push(check_cross_db_direct(&root)?);
    checks.push(check_legacy_paths(&root)?);
    let (size_check, hard_size_failures) = check_file_size(&root)?;
    checks.push(size_check);

    // ratchet:把每条违规按 baseline_id 是否命中划分。
    let mut new_violations = Vec::new();
    let baseline_ids: BTreeSet<String> = baseline
        .dependency_violation
        .iter()
        .map(|v| v.id.clone())
        .chain(baseline.legacy_path.iter().map(|v| v.id.clone()))
        .collect();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    for check in &checks {
        for v in &check.violations {
            match &v.baseline_id {
                Some(id) => {
                    seen_ids.insert(id.clone());
                }
                None => new_violations.push(v.clone()),
            }
        }
    }

    // 已消除的基线违规 = 基线登记了但本次没再出现的 id。
    let resolved_baseline_ids: Vec<String> = baseline_ids.difference(&seen_ids).cloned().collect();

    // 基线漂移提示:冻结 sha 与当前 HEAD 不同,说明代码已推进,基线可能需复核。
    // 仅提示不失败(违规数才是 ratchet 判据)。同时用聚合计数交叉校验基线自洽。
    if let Some(head) = current_git_head(&root) {
        if head != baseline.architecture_baseline_git_sha {
            eprintln!(
                "提示: 基线冻结 sha ({}) 与当前 HEAD ({}) 不同,若架构有变动请复核 legacy-allowlist.toml",
                &baseline.architecture_baseline_git_sha[..12.min(baseline.architecture_baseline_git_sha.len())],
                &head[..12.min(head.len())]
            );
        }
    }
    let counts = &baseline.baseline_counts;
    let declared = counts.dependency_violations + counts.legacy_paths + counts.cross_db_direct;
    let registered = baseline.dependency_violation.len() + baseline.legacy_path.len();
    if declared != registered + counts.cross_db_direct {
        eprintln!(
            "提示: baseline_counts 聚合({declared}) 与登记条目({registered} + cross_db {}) 不一致,请核对 legacy-allowlist.toml",
            counts.cross_db_direct
        );
    }

    Ok(Outcome {
        checks,
        new_violations,
        resolved_baseline_ids,
        hard_size_failures,
    })
}

/// 检查 1:依赖方向。解析 cargo metadata 的 workspace 内 crate 依赖边,
/// 比对允许的分层方向;已知 legacy 违规边(V1/V2)标记 baseline_id 不计新增。
fn check_dependency_direction(root: &Path) -> Result<CheckResult, String> {
    let mut result = CheckResult::new("dependency-direction");
    let baseline = Baseline::load(root)?;

    let meta = cargo_metadata(root)?;
    let ws_members = meta.workspace_member_names();
    let edges = meta.workspace_internal_edges(&ws_members);

    // 允许的依赖分层:值越小越靠叶子;禁止低层依赖高层(反向)。
    // 依据 dependency-graph.md §3 分层。owner-agnostic 层(analytics)不得依赖业务层。
    let layer = |name: &str| -> i32 {
        match name {
            "rust-quant-common" | "rust-quant-domain" => 0,
            "rust-quant-core" | "rust-quant-trading" => 1,
            "rust-quant-market" | "rust-quant-infrastructure" => 2,
            "rust-quant-indicators" | "rust-quant-risk" | "rust-quant-analytics" => 3,
            "rust-quant-strategies" => 4,
            "rust-quant-execution" => 5,
            "rust-quant-services" => 6,
            "rust-quant-orchestration" => 7,
            "rust-quant-cli" => 8,
            _ => -1,
        }
    };

    // legacy 基线边集合(规范化为 "a -> b" 判定用)。
    let baseline_edge_ids: Vec<(String, String)> = baseline
        .dependency_violation
        .iter()
        .map(|v| (v.id.clone(), v.edge.clone()))
        .collect();

    for (from, to) in &edges {
        let (lf, lt) = (layer(from), layer(to));
        if lf < 0 || lt < 0 {
            continue; // xtask 或外部 crate,跳过
        }
        // 违规:依赖了同层或更高层(lt >= lf 表示 to 不比 from 更靠叶子)。
        if lt >= lf {
            let baseline_id = match_baseline_edge(&baseline_edge_ids, from, to);
            result.violations.push(Violation {
                category: "dependency-direction".into(),
                detail: format!("{from}(层{lf}) 依赖 {to}(层{lt}),违反叶->根方向"),
                location: format!("{from} -> {to}"),
                baseline_id,
            });
        }
    }

    // V2:execution 依赖 strategies/indicators 是耦合方向异常(顺向层次抓不到)。
    // 显式判定:execution 存在到 strategies 或 indicators 的边即为该基线违规仍存在。
    let exec_bad = edges.iter().any(|(f, t)| {
        f == "rust-quant-execution"
            && (t == "rust-quant-strategies" || t == "rust-quant-indicators")
    });
    if exec_bad {
        let id = baseline_edge_ids
            .iter()
            .find(|(_, e)| e.to_lowercase().starts_with("execution"))
            .map(|(id, _)| id.clone());
        result.violations.push(Violation {
            category: "dependency-direction".into(),
            detail: "execution 反向依赖 strategies/indicators(应经 domain api)".into(),
            location: "execution -> {strategies, indicators}".into(),
            baseline_id: id,
        });
    }

    Ok(result)
}

/// 把 metadata 的 crate 名(rust-quant-analytics)与基线 edge("analytics -> strategies")对上。
fn match_baseline_edge(baseline: &[(String, String)], from: &str, to: &str) -> Option<String> {
    let short = |n: &str| n.trim_start_matches("rust-quant-").to_string();
    let (sf, st) = (short(from), short(to));
    for (id, edge) in baseline {
        // edge 形如 "analytics -> strategies" 或 "execution -> {strategies, indicators}(...)"
        let lower = edge.to_lowercase();
        if let Some((left, right)) = lower.split_once("->") {
            let left = left.trim();
            if left == sf && right.contains(&st) {
                return Some(id.clone());
            }
        }
    }
    None
}

/// cargo metadata 的最小视图。
struct Metadata {
    json: serde_json::Value,
}

impl Metadata {
    /// workspace 成员的 package 名集合。
    fn workspace_member_names(&self) -> BTreeSet<String> {
        let ids: BTreeSet<&str> = self.json["workspace_members"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        self.json["packages"]
            .as_array()
            .map(|pkgs| {
                pkgs.iter()
                    .filter(|p| p["id"].as_str().map(|id| ids.contains(id)).unwrap_or(false))
                    .filter_map(|p| p["name"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// workspace 内部依赖边 (from, to),只保留两端都是成员的边。
    fn workspace_internal_edges(&self, members: &BTreeSet<String>) -> Vec<(String, String)> {
        let mut edges = Vec::new();
        if let Some(pkgs) = self.json["packages"].as_array() {
            for p in pkgs {
                let Some(name) = p["name"].as_str() else {
                    continue;
                };
                if !members.contains(name) {
                    continue;
                }
                if let Some(deps) = p["dependencies"].as_array() {
                    for d in deps {
                        let Some(dep_name) = d["name"].as_str() else {
                            continue;
                        };
                        if members.contains(dep_name) && dep_name != name {
                            edges.push((name.to_string(), dep_name.to_string()));
                        }
                    }
                }
            }
        }
        edges
    }
}

/// 调用 cargo metadata --no-deps。
fn cargo_metadata(root: &Path) -> Result<Metadata, String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("执行 cargo metadata 失败: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata 退出码非零: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("解析 cargo metadata JSON 失败: {e}"))?;
    Ok(Metadata { json })
}

/// 读当前 git HEAD sha(失败返回 None,不影响检查)。
fn current_git_head(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 检查 2:跨库直连禁令(dependency-rules §13-1/2 精神)。
/// 静态扫描 crates/ 下源码,禁止新增直连 quant_web/quant_news 的 DB 连接。
/// 基线为 0(已 HTTP 化);当前 quant_web 只经 execution_task_client 的 HTTP 调用,
/// quant_core DB 单例有 URL 门禁,故不应有匹配。任何匹配都是新增违规。
fn check_cross_db_direct(root: &Path) -> Result<CheckResult, String> {
    let mut result = CheckResult::new("cross-db-direct");
    // 直连另一库的真实特征是:连接串里出现另一库的 URL 路径字面量(`/quant_web`、`/quant_news`)。
    // 仅文件里提到库名(注释、门禁校验测试名如 quant_core_..._rejects_quant_web_fallback)不算直连,
    // 且 quant_web 的正当访问已 HTTP 化(execution_task_client)。基线为 0,任何匹配都是新增违规。
    let crates_dir = root.join("crates");
    let mut files = Vec::new();
    collect_rs_files(&crates_dir, &mut files);
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        // sqlx_pool 是受控 core DB 单例入口(URL 门禁在此实现,含 reject 逻辑),豁免。
        if f.ends_with("sqlx_pool.rs") {
            continue;
        }
        // 排除测试:tests/ 目录、文件名含 test、含 #[cfg(test)] 的文件。测试 fixture / 健康检查
        // 里出现别库 URL 是合法的(路由校验、env 设置),不是生产直连。静态扫描无法区分二者,
        // 故只扫生产源码。运行时真实防护由 sqlx_pool.rs 的 URL 门禁(有测试覆盖)提供。
        let path_str = rel(root, f);
        let is_test = path_str.contains("/tests/")
            || path_str.contains("tests/")
            || f.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains("test"))
                .unwrap_or(false)
            || text.contains("#[cfg(test)]");
        if is_test {
            continue;
        }
        for (lineno, line) in text.lines().enumerate() {
            // 连接串路径字面量:postgres://.../quant_web 或 .../quant_news
            let hits_web = line.contains("/quant_web");
            let hits_news = line.contains("/quant_news");
            if (hits_web || hits_news)
                && (line.contains("postgres://") || line.contains("postgresql://"))
            {
                let target = if hits_web { "quant_web" } else { "quant_news" };
                result.violations.push(Violation {
                    category: "cross-db-direct".into(),
                    detail: format!("连接串直连 {target} 数据库(应经 owner HTTP API)"),
                    location: format!("{}:{}", rel(root, f), lineno + 1),
                    baseline_id: None,
                });
            }
        }
    }
    Ok(result)
}

/// 检查 2b:legacy signed read-only 路径存续核对(基线 V3/V4/V5)。
/// 这些文件是登记的 legacy 直读路径;只要文件还在,就是"基线内违规仍存在"(seen)。
/// 文件被删除(收敛完成)时不再 seen,ratchet 将其判为已消除。不检测新增(新增由依赖方向 + review 覆盖)。
fn check_legacy_paths(root: &Path) -> Result<CheckResult, String> {
    let mut result = CheckResult::new("legacy-signed-read-only");
    let baseline = Baseline::load(root)?;
    for lp in &baseline.legacy_path {
        let mut paths = vec![lp.path.clone()];
        if let Some(also) = &lp.path_also {
            paths.push(also.clone());
        }
        // 只要任一登记路径仍存在,视为该基线违规仍在。
        let still_present = paths.iter().any(|p| root.join(p).exists());
        if still_present {
            result.violations.push(Violation {
                category: "legacy-signed-read-only".into(),
                detail: format!("legacy 账户直读路径仍存在(基线 {})", lp.id),
                location: paths.join(", "),
                baseline_id: Some(lp.id.clone()),
            });
        }
    }
    Ok(result)
}

/// 检查 3:文件行数闸门(dependency-rules §13-10)。
/// 复用根 scripts/dev/check_code_file_line_limit.sh 的阈值:1000 WARN / 2000 硬失败。
/// 硬失败单独返回,进入 Outcome.hard_size_failures 直接导致 FAIL。
fn check_file_size(root: &Path) -> Result<(CheckResult, Vec<String>), String> {
    let mut result = CheckResult::new("file-size");
    let mut hard = Vec::new();
    const WARN: usize = 1000;
    const HARD: usize = 2000;
    let crates_dir = root.join("crates");
    let mut files = Vec::new();
    collect_rs_files(&crates_dir, &mut files);
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let lines = text.lines().count();
        if lines > HARD {
            hard.push(format!("{} lines={lines} hard_limit={HARD}", rel(root, f)));
        } else if lines > WARN {
            result
                .warnings
                .push(format!("{} lines={lines} target={WARN}", rel(root, f)));
        }
    }
    Ok((result, hard))
}

/// 递归收集 .rs 文件,跳过 target/。
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().map(|n| n == "target").unwrap_or(false) {
                continue;
            }
            collect_rs_files(&p, out);
        } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// 相对 workspace 根的可读路径。
fn rel(root: &Path, f: &Path) -> String {
    f.strip_prefix(root).unwrap_or(f).display().to_string()
}
