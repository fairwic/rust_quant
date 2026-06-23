use anyhow::{anyhow, Context, Result};
use rust_quant_services::strategy::pre_major_listing_perp_catchup::{
    evaluate_listing_catchup_paper, ListingCatchupAcceptanceCriteria, ListingCatchupPaperSample,
};
use serde::Deserialize;
use std::fs;
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PaperAcceptanceInput {
    Samples(Vec<ListingCatchupPaperSample>),
    Envelope {
        samples: Vec<ListingCatchupPaperSample>,
        #[serde(default)]
        criteria: Option<ListingCatchupAcceptanceCriteria>,
    },
}
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
fn main() -> Result<()> {
    let args = parse_args()?;
    let input = read_input(&args.input_path)?;
    let criteria = ListingCatchupAcceptanceCriteria {
        min_trade_samples: args
            .min_trade_samples
            .unwrap_or_else(|| input.criteria.clone().unwrap_or_default().min_trade_samples),
        min_win_rate_pct: args
            .min_win_rate_pct
            .unwrap_or_else(|| input.criteria.clone().unwrap_or_default().min_win_rate_pct),
        require_positive_total_net_return: input
            .criteria
            .unwrap_or_default()
            .require_positive_total_net_return,
    };
    let report = evaluate_listing_catchup_paper(input.samples, criteria);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.production_status != "paper_ready" {
        return Err(anyhow!(
            "pre_major_listing_perp_catchup paper acceptance blocked: {}",
            report.blockers.join(",")
        ));
    }
    Ok(())
}
struct CliArgs {
    /// input路径，用于当前结构体的业务数据。
    input_path: String,
    /// 最小tradesamples；为空时表示该条件不启用。
    min_trade_samples: Option<usize>,
    /// 最小胜率百分比。
    min_win_rate_pct: Option<f64>,
}
struct ParsedInput {
    /// 列表数据。
    samples: Vec<ListingCatchupPaperSample>,
    /// criteria；为空时表示该条件不启用。
    criteria: Option<ListingCatchupAcceptanceCriteria>,
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
fn parse_args() -> Result<CliArgs> {
    let mut input_path = std::env::var("PRE_MAJOR_LISTING_PAPER_INPUT").ok();
    let mut min_trade_samples = std::env::var("PRE_MAJOR_LISTING_MIN_TRADE_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    let mut min_win_rate_pct = std::env::var("PRE_MAJOR_LISTING_MIN_WIN_RATE_PCT")
        .ok()
        .and_then(|value| value.parse::<f64>().ok());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input_path = args.next(),
            "--min-trade-samples" => {
                min_trade_samples = args.next().and_then(|value| value.parse::<usize>().ok());
            }
            "--min-win-rate-pct" => {
                min_win_rate_pct = args.next().and_then(|value| value.parse::<f64>().ok());
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    let input_path = input_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing --input or PRE_MAJOR_LISTING_PAPER_INPUT"))?;
    Ok(CliArgs {
        input_path,
        min_trade_samples,
        min_win_rate_pct,
    })
}
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
fn read_input(path: &str) -> Result<ParsedInput> {
    let body = fs::read_to_string(path).with_context(|| format!("read paper input: {path}"))?;
    match serde_json::from_str::<PaperAcceptanceInput>(&body)
        .with_context(|| format!("parse paper input JSON: {path}"))?
    {
        PaperAcceptanceInput::Samples(samples) => Ok(ParsedInput {
            samples,
            criteria: None,
        }),
        PaperAcceptanceInput::Envelope { samples, criteria } => {
            Ok(ParsedInput { samples, criteria })
        }
    }
}
/// 执行输出usage步骤，串起量化核心需要的状态推进和错误处理。
fn print_usage() {
    println!(
        "Usage: pre_major_listing_paper_acceptance --input <samples.json> [--min-trade-samples 30] [--min-win-rate-pct 60]"
    );
}
