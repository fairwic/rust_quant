use anyhow::{bail, Context, Result};
use rust_quant_cli::app::tradingview_velocity_parity::strict_static_universe_io::{
    audit_and_seal_strict_static_plan_from_quant_core, decode_and_validate_sealed_snapshot,
    decode_and_validate_selection_plan_from_snapshot, freeze_strict_static_selection_plan,
    reaudit_sealed_snapshot_from_quant_core, StrictStaticSelectionPlanV2,
    StrictStaticSnapshotBuildArgs,
};
use std::path::PathBuf;

/// 冻结器只允许新建计划或复用已保存计划，两种输入不能混合。
#[derive(Debug, PartialEq, Eq)]
enum FreezeInput {
    Fresh(StrictStaticSnapshotBuildArgs),
    SavedSelectionSnapshot(PathBuf),
}

/// Research-only 冻结器参数；研究窗口和输出位置必须由调用方显式给出。
#[derive(Debug)]
struct Args {
    input: FreezeInput,
    output: PathBuf,
}

/// 固定 current-live Top60 selection plan，并只在同源数据达到 60/60 时 seal。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_args(std::env::args().skip(1))?;
    let snapshot = match args.input {
        FreezeInput::Fresh(build_args) => {
            let plan = freeze_strict_static_selection_plan(build_args).await?;
            // 成员一旦从 OKX 响应冻结就先落盘；后续 Core 连接或覆盖审计失败也不能丢失计划。
            write_selection_plan_checkpoint(&args.output, &plan)?;
            audit_and_seal_strict_static_plan_from_quant_core(plan).await?
        }
        FreezeInput::SavedSelectionSnapshot(path) => {
            let raw = std::fs::read(&path)
                .with_context(|| format!("读取已保存冻结快照失败：{}", path.display()))?;
            let root: serde_json::Value =
                serde_json::from_slice(&raw).context("解析已保存冻结快照根节点失败")?;
            if root.get("sealed").and_then(serde_json::Value::as_bool) == Some(true) {
                let saved = decode_and_validate_sealed_snapshot(&raw)?;
                reaudit_sealed_snapshot_from_quant_core(&saved).await?
            } else {
                let plan = decode_and_validate_selection_plan_from_snapshot(&raw)?;
                audit_and_seal_strict_static_plan_from_quant_core(plan).await?
            }
        }
    };
    write_json(&args.output, &snapshot)?;
    println!(
        "{} sealed={} complete_members={}/60",
        args.output.display(),
        snapshot.sealed,
        snapshot.complete_member_count
    );
    Ok(())
}

/// 在任何 K 线审计前保存不可替换成员，使失败重试仍消费同一 selection plan。
fn write_selection_plan_checkpoint(
    output: &std::path::Path,
    plan: &StrictStaticSelectionPlanV2,
) -> Result<()> {
    write_json(
        output,
        &serde_json::json!({
            "snapshot_stage": "selection_plan_frozen",
            "selection_plan": plan,
            "sealed": false
        }),
    )
}

/// 统一创建父目录并写入格式化 JSON。
fn write_json(output: &std::path::Path, value: &impl serde::Serialize) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .context("序列化 surviving static Top60 冻结审计结果失败")?;
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建输出目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{json}\n"))
        .with_context(|| format!("写入冻结审计结果失败：{}", output.display()))
}

/// 只接受最小显式参数集；未知开关直接失败以防研究口径漂移。
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut universe_version = None;
    let mut evaluation_start_ms = None;
    let mut evaluation_end_exclusive_ms = None;
    let mut proxy_url = None;
    let mut selection_snapshot = None;
    let mut output = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--universe-version" => {
                universe_version =
                    Some(args.next().context("--universe-version requires a value")?);
            }
            "--evaluation-start-ms" => {
                evaluation_start_ms = Some(parse_i64(
                    "--evaluation-start-ms",
                    args.next()
                        .context("--evaluation-start-ms requires Unix milliseconds")?,
                )?);
            }
            "--evaluation-end-exclusive-ms" => {
                evaluation_end_exclusive_ms = Some(parse_i64(
                    "--evaluation-end-exclusive-ms",
                    args.next()
                        .context("--evaluation-end-exclusive-ms requires Unix milliseconds")?,
                )?);
            }
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().context("--output requires a file path")?,
                ));
            }
            "--proxy-url" => {
                proxy_url = Some(args.next().context("--proxy-url requires a URL")?);
            }
            "--selection-snapshot" => {
                selection_snapshot = Some(PathBuf::from(
                    args.next()
                        .context("--selection-snapshot requires a file path")?,
                ));
            }
            "--help" | "-h" => {
                println!(
                    "Usage:\n  tradingview_velocity_freeze_top60 \
                     --universe-version VERSION --evaluation-start-ms UNIX_MS \
                     --evaluation-end-exclusive-ms UNIX_MS --output PATH [--proxy-url URL]\n  \
                     tradingview_velocity_freeze_top60 \
                     --selection-snapshot PATH --output PATH"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    let has_fresh_argument = universe_version.is_some()
        || evaluation_start_ms.is_some()
        || evaluation_end_exclusive_ms.is_some()
        || proxy_url.is_some();
    let input = match (selection_snapshot, has_fresh_argument) {
        (Some(_), true) => {
            bail!("--selection-snapshot 与新冻结窗口参数/--proxy-url 互斥")
        }
        (Some(path), false) => FreezeInput::SavedSelectionSnapshot(path),
        (None, _) => FreezeInput::Fresh(StrictStaticSnapshotBuildArgs {
            universe_version: universe_version.context("--universe-version is required")?,
            evaluation_start_ms: evaluation_start_ms
                .context("--evaluation-start-ms is required")?,
            evaluation_end_exclusive_ms: evaluation_end_exclusive_ms
                .context("--evaluation-end-exclusive-ms is required")?,
            proxy_url,
        }),
    };
    Ok(Args {
        input,
        output: output.context("--output is required")?,
    })
}

/// 单独标注参数名，避免毫秒解析错误失去上下文。
fn parse_i64(name: &str, value: String) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("{name} must be a signed 64-bit integer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_requires_explicit_window_identity_and_output() {
        let args = parse_args([
            "--universe-version".to_owned(),
            "surviving_static_top60_test".to_owned(),
            "--evaluation-start-ms".to_owned(),
            "8640000000".to_owned(),
            "--evaluation-end-exclusive-ms".to_owned(),
            "8726400000".to_owned(),
            "--output".to_owned(),
            "out.json".to_owned(),
        ])
        .unwrap();

        assert!(matches!(
            args.input,
            FreezeInput::Fresh(StrictStaticSnapshotBuildArgs {
                ref universe_version,
                ..
            }) if universe_version == "surviving_static_top60_test"
        ));
        assert_eq!(args.output, PathBuf::from("out.json"));
        assert!(parse_args(Vec::<String>::new()).is_err());
    }

    #[test]
    fn parser_accepts_saved_plan_but_rejects_mixed_selection_inputs() {
        let saved = parse_args([
            "--selection-snapshot".to_owned(),
            "initial.json".to_owned(),
            "--output".to_owned(),
            "sealed.json".to_owned(),
        ])
        .unwrap();
        assert_eq!(
            saved.input,
            FreezeInput::SavedSelectionSnapshot(PathBuf::from("initial.json"))
        );

        assert!(parse_args([
            "--selection-snapshot".to_owned(),
            "initial.json".to_owned(),
            "--universe-version".to_owned(),
            "must-not-mix".to_owned(),
            "--output".to_owned(),
            "sealed.json".to_owned(),
        ])
        .is_err());
        assert!(parse_args([
            "--selection-snapshot".to_owned(),
            "initial.json".to_owned(),
            "--proxy-url".to_owned(),
            "http://127.0.0.1:1080".to_owned(),
            "--output".to_owned(),
            "sealed.json".to_owned(),
        ])
        .is_err());
    }
}
