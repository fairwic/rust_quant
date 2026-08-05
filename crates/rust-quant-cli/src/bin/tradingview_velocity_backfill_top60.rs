use anyhow::{bail, Context, Result};
use rust_quant_cli::app::okx_historical_15m_backfill::{
    run_historical_15m_backfill_for_symbols, Historical15mCoverageWindow,
    Historical15mFrozenSymbolsBackfillArgs,
};
use rust_quant_cli::app::tradingview_velocity_parity::strict_static_universe_io::decode_and_validate_selection_plan_from_snapshot;
use std::path::PathBuf;

const DEFAULT_OKX_BASE: &str = "https://www.okx.com";

/// 只补已冻结 60 成员和计划半开窗口；默认 dry-run，不能自行选择替补币。
#[derive(Debug, PartialEq, Eq)]
struct Args {
    selection_snapshot: PathBuf,
    download_concurrency: usize,
    batch_size: usize,
    write: bool,
    okx_base: String,
    proxy_url: Option<String>,
}

/// 对严格 selection plan 中的原始成员执行同源历史 15m 补数。
#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = parse_args(std::env::args().skip(1))?;
    let raw = std::fs::read(&args.selection_snapshot).with_context(|| {
        format!(
            "读取 surviving static Top60 selection snapshot 失败：{}",
            args.selection_snapshot.display()
        )
    })?;
    let plan = decode_and_validate_selection_plan_from_snapshot(&raw)?;
    let symbols = plan
        .members
        .iter()
        .map(|member| member.symbol.clone())
        .collect::<Vec<_>>();
    let window =
        Historical15mCoverageWindow::new(plan.warmup_start_ms, plan.evaluation_end_exclusive_ms)?;
    let report = run_historical_15m_backfill_for_symbols(
        &Historical15mFrozenSymbolsBackfillArgs {
            download_concurrency: args.download_concurrency,
            batch_size: args.batch_size,
            write: args.write,
            strict: true,
            okx_base: args.okx_base,
            proxy_url: args.proxy_url,
        },
        &symbols,
        window,
    )
    .await?;
    println!(
        "tradingview_velocity_backfill_top60: symbols={} archives={} candles_15m={} rows_upserted={} coverage_audited_symbols={} dry_run={}",
        report.symbols,
        report.archive_files,
        report.candles_15m,
        report.rows_upserted,
        report.coverage_audited_symbols,
        report.dry_run
    );
    Ok(())
}

/// 解析最小补数参数；窗口和成员只能来自 selection snapshot。
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut selection_snapshot = None;
    let mut download_concurrency = 8usize;
    let mut batch_size = 500usize;
    let mut write_mode = None;
    let mut okx_base = DEFAULT_OKX_BASE.to_owned();
    let mut proxy_url = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--selection-snapshot" => {
                selection_snapshot = Some(PathBuf::from(
                    args.next()
                        .context("--selection-snapshot requires a file path")?,
                ));
            }
            "--download-concurrency" => {
                download_concurrency = args
                    .next()
                    .context("--download-concurrency requires a value")?
                    .parse()
                    .context("parse --download-concurrency")?;
            }
            "--batch-size" => {
                batch_size = args
                    .next()
                    .context("--batch-size requires a value")?
                    .parse()
                    .context("parse --batch-size")?;
            }
            "--write" => set_write_mode(&mut write_mode, true)?,
            "--dry-run" => set_write_mode(&mut write_mode, false)?,
            "--okx-base" => {
                okx_base = args
                    .next()
                    .context("--okx-base requires a URL")?
                    .trim_end_matches('/')
                    .to_owned();
            }
            "--proxy-url" => {
                proxy_url = Some(args.next().context("--proxy-url requires a URL")?);
            }
            "--help" | "-h" => {
                println!(
                    "Usage: tradingview_velocity_backfill_top60 \
                     --selection-snapshot PATH [--download-concurrency 8] [--batch-size 500] \
                     [--okx-base URL] [--proxy-url URL] [--dry-run|--write]"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    if download_concurrency == 0 || download_concurrency > 16 {
        bail!("--download-concurrency must be between 1 and 16");
    }
    if batch_size == 0 || batch_size > 2_000 {
        bail!("--batch-size must be between 1 and 2000");
    }
    if okx_base.trim().is_empty() {
        bail!("--okx-base cannot be empty");
    }
    Ok(Args {
        selection_snapshot: selection_snapshot.context("--selection-snapshot is required")?,
        download_concurrency,
        batch_size,
        write: write_mode.unwrap_or(false),
        okx_base,
        proxy_url,
    })
}

/// 显式拒绝同时声明 dry-run 与 write，避免命令顺序改变写入语义。
fn set_write_mode(mode: &mut Option<bool>, write: bool) -> Result<()> {
    if mode.replace(write).is_some() {
        bail!("--dry-run 与 --write 只能出现一次且不能同时使用");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_defaults_to_dry_run_and_requires_saved_plan() {
        let args =
            parse_args(["--selection-snapshot".to_owned(), "initial.json".to_owned()]).unwrap();

        assert!(!args.write);
        assert_eq!(args.download_concurrency, 8);
        assert_eq!(args.batch_size, 500);
        assert!(parse_args(Vec::<String>::new()).is_err());
    }

    #[test]
    fn parser_requires_a_single_explicit_write_mode() {
        assert!(parse_args([
            "--selection-snapshot".to_owned(),
            "initial.json".to_owned(),
            "--write".to_owned(),
            "--dry-run".to_owned(),
        ])
        .is_err());

        let args = parse_args([
            "--selection-snapshot".to_owned(),
            "initial.json".to_owned(),
            "--write".to_owned(),
        ])
        .unwrap();
        assert!(args.write);
    }
}
