use anyhow::{anyhow, Context, Result};
use rust_quant_services::strategy::pre_major_listing_perp_catchup::{
    build_listing_catchup_paper_sample, ListingCatchupPaperProbeSeed, ListingCatchupPaperSample,
};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProbeInput {
    Seeds(Vec<ListingCatchupPaperProbeSeed>),
    Envelope {
        seeds: Vec<ListingCatchupPaperProbeSeed>,
    },
}

#[derive(Debug, Serialize)]
struct ProbeOutput {
    samples: Vec<ListingCatchupPaperSample>,
    production_note: &'static str,
}

fn main() -> Result<()> {
    let input_path = parse_input_path()?;
    let seeds = read_input(&input_path)?;
    let samples = seeds
        .into_iter()
        .map(build_listing_catchup_paper_sample)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| anyhow!("build paper sample from probe seed failed: {error}"))?;

    println!(
        "{}",
        serde_json::to_string_pretty(&ProbeOutput {
            samples,
            production_note: "paper_samples_only_live_trading_disabled",
        })?
    );
    Ok(())
}

fn parse_input_path() -> Result<String> {
    let mut input_path = std::env::var("PRE_MAJOR_LISTING_PROBE_INPUT").ok();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input_path = args.next(),
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    input_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing --input or PRE_MAJOR_LISTING_PROBE_INPUT"))
}

fn read_input(path: &str) -> Result<Vec<ListingCatchupPaperProbeSeed>> {
    let body = fs::read_to_string(path).with_context(|| format!("read probe input: {path}"))?;
    match serde_json::from_str::<ProbeInput>(&body)
        .with_context(|| format!("parse probe input JSON: {path}"))?
    {
        ProbeInput::Seeds(seeds) => Ok(seeds),
        ProbeInput::Envelope { seeds } => Ok(seeds),
    }
}

fn print_usage() {
    println!("Usage: pre_major_listing_paper_samples_from_probe --input <probe-seeds.json>");
}
