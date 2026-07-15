use anyhow::{anyhow, Context, Result};
use rust_quant_analytics::pa_quant_tree::SourceIdentity;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCE_SCOPES: [&str; 4] = [
    "crates/analytics/src/pa_quant_tree",
    "crates/strategies/src/implementations/pa_quant_tree",
    "crates/rust-quant-cli/src/bin/pa_quant_tree_15m_research.rs",
    "crates/rust-quant-cli/src/bin/pa_quant_tree_15m_research",
];

/// 绑定 PA analytics、策略实现和研究 CLI 的 Git HEAD、内容指纹与 scoped dirty 状态。
pub(crate) fn detect_source_identity(repo_root: &Path) -> Result<SourceIdentity> {
    let mut sources = Vec::new();
    for scope in SOURCE_SCOPES {
        collect_rust_sources(repo_root, Path::new(scope), &mut sources)?;
    }
    sources.sort();
    sources.dedup();
    anyhow::ensure!(
        !sources.is_empty(),
        "PA source identity found no Rust sources"
    );

    let git_head = git_output(repo_root, &["rev-parse", "HEAD"])?;
    let source_fingerprint = fingerprint_sources(repo_root, &sources)?;
    let dirty = scoped_git_status(repo_root)?;
    Ok(SourceIdentity {
        git_head,
        source_fingerprint,
        dirty,
    })
}

/// 从任意工作目录解析 owning Git 仓库根目录，供独立二进制稳定定位源码 scope。
pub(crate) fn discover_repo_root(start: &Path) -> Result<PathBuf> {
    let value = git_output(start, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(value))
}

/// 递归收集固定 scope 内的 Rust 文件，输出始终使用仓库相对路径。
fn collect_rust_sources(
    repo_root: &Path,
    relative: &Path,
    sources: &mut Vec<PathBuf>,
) -> Result<()> {
    let absolute = repo_root.join(relative);
    if !absolute.exists() {
        return Ok(());
    }
    if absolute.is_file() {
        if absolute
            .extension()
            .is_some_and(|extension| extension == "rs")
        {
            sources.push(relative.to_path_buf());
        }
        return Ok(());
    }

    for entry in fs::read_dir(&absolute)
        .with_context(|| format!("failed to read source scope {}", absolute.display()))?
    {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        collect_rust_sources(repo_root, &child_relative, sources)?;
    }
    Ok(())
}

/// 以路径和内容长度分隔每个文件，避免拼接边界歧义并保证遍历顺序稳定。
fn fingerprint_sources(repo_root: &Path, sources: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha256::new();
    for relative in sources {
        let path = relative
            .to_str()
            .with_context(|| format!("non UTF-8 source path: {}", relative.display()))?;
        let content = fs::read(repo_root.join(relative))
            .with_context(|| format!("failed to read source: {path}"))?;
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(content);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

/// 查询固定源码 scope 的 Git 状态，避免无关文档或其他模块污染研究 dirty 标记。
fn scoped_git_status(repo_root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain", "--"])
        .args(SOURCE_SCOPES)
        .output()
        .context("failed to inspect scoped Git status")?;
    if !output.status.success() {
        return Err(anyhow!(
            "scoped Git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(!output.stdout.is_empty())
}

/// 执行只读 Git 查询并返回去除换行的标准输出。
fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .context("failed to execute Git identity query")?;
    if !output.status.success() {
        return Err(anyhow!(
            "Git identity query failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value = String::from_utf8(output.stdout)?.trim().to_owned();
    anyhow::ensure!(
        !value.is_empty(),
        "Git identity query returned an empty value"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 自动清理源码身份测试使用的临时 Git 仓库。
    struct TestRepo {
        root: PathBuf,
    }

    impl TestRepo {
        /// 以指定文件创建并提交最小仓库；输入顺序用于模拟不同目录遍历顺序。
        fn create(label: &str, files: &[(&str, &str)]) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "pa-source-identity-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            for (path, content) in files {
                let absolute = root.join(path);
                fs::create_dir_all(absolute.parent().unwrap()).unwrap();
                fs::write(absolute, content).unwrap();
            }
            run_git(&root, &["init", "-q"]);
            run_git(&root, &["add", "."]);
            run_git(
                &root,
                &[
                    "-c",
                    "user.name=PA Test",
                    "-c",
                    "user.email=pa-test@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
            );
            Self { root }
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// 执行临时仓库初始化所需的受限 Git 命令。
    fn run_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn fingerprint_is_order_independent_and_changes_with_content() {
        let files = [
            (
                "crates/analytics/src/pa_quant_tree/zeta.rs",
                "pub fn zeta() {}\n",
            ),
            (
                "crates/strategies/src/implementations/pa_quant_tree/alpha.rs",
                "pub fn alpha() {}\n",
            ),
        ];
        let forward = TestRepo::create("forward", &files);
        let reverse = TestRepo::create("reverse", &[files[1], files[0]]);

        let forward_identity = detect_source_identity(&forward.root).unwrap();
        let reverse_identity = detect_source_identity(&reverse.root).unwrap();
        assert_eq!(
            forward_identity.source_fingerprint,
            reverse_identity.source_fingerprint
        );
        assert!(!forward_identity.dirty);

        fs::write(
            forward
                .root
                .join("crates/analytics/src/pa_quant_tree/zeta.rs"),
            "pub fn zeta_changed() {}\n",
        )
        .unwrap();
        let changed = detect_source_identity(&forward.root).unwrap();
        assert_ne!(
            forward_identity.source_fingerprint,
            changed.source_fingerprint
        );
        assert!(changed.dirty);
    }
}
