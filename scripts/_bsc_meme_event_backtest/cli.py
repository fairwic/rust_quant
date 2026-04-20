from __future__ import annotations

import argparse
import csv
import json
import os
import time
from dataclasses import asdict
from pathlib import Path

from .chainstack import resolve_bsc_rpc_url
from .cex_flow import fetch_cex_counterparty_candidates
from .core import BacktestConfig, BacktestResult, run_price_volume_replay, summarize_results
from .event import run_full_event_replay
from .event_sources import collect_event_snapshot
from .lbank import default_samples, discover_first_candle, fetch_lbank_klines

SUCCESS_SYMBOLS = {
    "rave_usdt",
    "bianrensheng_usdt",
    "4_usdt",
    "palu_usdt",
    "kefuxiaohe_usdt",
    "xiuxian_usdt",
    "hajimi_usdt",
}


def write_outputs(out_dir: Path, results: list[BacktestResult], summary: dict) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False))
    (out_dir / "report.md").write_text(render_report(results, summary))
    if not results:
        return
    with (out_dir / "trades.tsv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(asdict(results[0]).keys()), delimiter="\t")
        writer.writeheader()
        for result in results:
            writer.writerow(asdict(result))


def render_report(results: list[BacktestResult], summary: dict) -> str:
    verdict = "PASS" if summary["passes_proof_gate"] else "FAIL"
    lines = [
        "# BSC Meme Event Replay Report",
        "",
        f"- Verdict: {verdict}",
        f"- Data source: {summary['data_source']}",
        f"- Proof scope: {summary['proof_scope']}",
        f"- Samples: {summary['samples']}",
        f"- Trades: {summary['trades']}",
        f"- Win rate: {summary['win_rate']:.2%}",
        f"- Net R: {summary['net_r']:.4f}",
        f"- Avg net R: {summary['avg_net_r']:.4f}",
        f"- Profit factor: {summary['profit_factor']:.4f}",
        f"- Net R without largest winner: {summary['net_r_without_largest_winner']:.4f}",
        "",
        replay_scope_note(summary),
        "",
        "| Symbol | Entered | Exit | Net R | Bars | Warning |",
        "| --- | --- | --- | ---: | ---: | --- |",
    ]
    for r in results:
        lines.append(
            f"| {r.symbol} | {r.entered} | {r.exit_reason} | {r.net_r:.4f} | "
            f"{r.bars} | {r.data_warning or ''} |"
        )
    lines.extend(
        [
            "",
            "Proof gate requires: trades >= 10, win rate >= 42%, avg win >= 2R,",
            "avg loss <= 1R, avg net >= 0.25R, profit factor >= 1.35, and positive",
            "net R after removing the largest winner.",
        ]
    )
    return "\n".join(lines) + "\n"


def replay_scope_note(summary: dict) -> str:
    if summary["proof_scope"] == "full_event_public_data_strict":
        coverage = summary.get("event_data_coverage", {})
        return (
            "This strict replay requires security, DEX liquidity, derivatives, "
            "historical OI growth, and CEX wallet-flow evidence before trading. "
            f"Coverage: {json.dumps(coverage, ensure_ascii=False)}."
        )
    return (
        "This replay uses LBank public 5m klines only. It does not prove the full "
        "strategy because OI, funding, depth, security, and wallet-flow fields are "
        "missing for most samples."
    )


def run_online(args) -> tuple[list[BacktestResult], dict]:
    cfg = BacktestConfig()
    start_ts = int(args.start_ts)
    end_ts = int(args.end_ts or time.time())
    raw_dir = Path(args.out_dir) / "raw_lbank_5m"
    event_raw_dir = Path(args.out_dir) / "raw_event_snapshots"
    candidate_dir = Path(args.out_dir) / "cex_counterparty_candidates"
    raw_dir.mkdir(parents=True, exist_ok=True)
    event_raw_dir.mkdir(parents=True, exist_ok=True)
    if args.discover_cex_candidates:
        candidate_dir.mkdir(parents=True, exist_ok=True)
    bsc_rpc_url = resolve_bsc_rpc_url(
        args.bsc_rpc_url, args.bsc_rpc_env, args.chainstack_key_env
    )
    results: list[BacktestResult] = []
    event_snapshots = []
    for sample in default_samples():
        symbol = sample["symbol"]
        first_ts = discover_first_candle(symbol, start_ts, end_ts)
        if first_ts is None:
            results.append(BacktestResult(symbol=symbol, data_warning="NO_LBANK_DATA"))
            continue
        last_ts = min(end_ts, first_ts + args.days * 86400)
        candles = fetch_lbank_klines(symbol, first_ts, last_ts)
        (raw_dir / f"{symbol}.json").write_text(json.dumps([asdict(c) for c in candles]))
        if args.mode == "full-event":
            event_snapshot, raw_event = collect_event_snapshot(
                sample,
                first_ts,
                last_ts,
                os.environ.get(args.coinalyze_key_env),
                args.coinalyze_interval,
                os.environ.get(args.etherscan_key_env),
                args.cex_labels_path,
                candles[-1].close if candles else 0.0,
                bsc_rpc_url,
            )
            event_snapshots.append(event_snapshot)
            (event_raw_dir / f"{symbol}.json").write_text(json.dumps(raw_event, ensure_ascii=False))
            if args.discover_cex_candidates:
                candidates = fetch_cex_counterparty_candidates(
                    sample,
                    os.environ.get(args.etherscan_key_env),
                    first_ts,
                    last_ts,
                    candles[-1].close if candles else 0.0,
                    args.cex_candidate_limit,
                    bsc_rpc_url,
                )
                write_cex_candidates(candidate_dir / f"{symbol}.tsv", candidates)
            result = run_full_event_replay(
                symbol, candles, event_snapshot, cfg, strict=not args.allow_partial_event_data
            )
        else:
            result = run_price_volume_replay(symbol, candles, cfg)
        if args.mode == "price-volume" and symbol in SUCCESS_SYMBOLS:
            result.data_warning = "PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY"
        results.append(result)
    summary = summarize_results(results)
    summary["data_source"] = data_source_name(
        args.mode, args.coinalyze_key_env, args.etherscan_key_env, args.chainstack_key_env
    )
    summary["proof_scope"] = proof_scope_name(args.mode, args.allow_partial_event_data)
    summary["sample_plan"] = default_samples()
    if event_snapshots:
        summary["event_data_coverage"] = event_data_coverage(event_snapshots)
    return results, summary


def data_source_name(
    mode: str,
    coinalyze_key_env: str = "COINALYZE_API_KEY",
    etherscan_key_env: str = "ETHERSCAN_API_KEY",
    chainstack_key_env: str = "CHAINSTACK_API_KEY",
) -> str:
    if mode == "full-event":
        return (
            "LBank 5m klines + GoPlus security/liquidity + "
            f"Bitget/Gate derivatives + Coinalyze history via ${coinalyze_key_env} + "
            f"BSC RPC token flow via --bsc-rpc-url/$BSC_RPC_URL or Chainstack "
            f"${chainstack_key_env}, with Etherscan fallback via ${etherscan_key_env}"
        )
    return "LBank public 5m klines"


def proof_scope_name(mode: str, allow_partial: bool) -> str:
    if mode == "full-event" and allow_partial:
        return "full_event_public_data_partial"
    if mode == "full-event":
        return "full_event_public_data_strict"
    return "price_volume_only_not_full_strategy"


def event_data_coverage(snapshots) -> dict:
    total = len(snapshots)
    if total == 0:
        return {}
    return {
        "samples": total,
        "contract_address": sum(1 for s in snapshots if s.contract_address),
        "security_checked": sum(1 for s in snapshots if s.security_checked),
        "dex_liquidity_checked": sum(1 for s in snapshots if s.dex_liquidity_usd > 0),
        "derivatives_checked": sum(1 for s in snapshots if s.derivatives_checked),
        "historical_oi_growth": sum(1 for s in snapshots if s.historical_oi_available),
        "cex_flow_checked": sum(1 for s in snapshots if s.cex_flow_checked),
    }


def write_cex_candidates(path: Path, payload: dict) -> None:
    with path.open("w", newline="") as f:
        fieldnames = [
            "address",
            "sent_tokens",
            "received_tokens",
            "net_received_tokens",
            "volume_tokens",
            "volume_usd",
            "net_received_usd",
            "tx_count",
        ]
        writer = csv.DictWriter(f, fieldnames=fieldnames, delimiter="\t")
        writer.writeheader()
        if not payload.get("available"):
            writer.writerow({"address": payload.get("error", "UNAVAILABLE")})
            return
        for row in payload.get("candidates", []):
            writer.writerow({key: row.get(key, "") for key in fieldnames})


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--start-ts", default=1758758400)
    parser.add_argument("--end-ts", default=None)
    parser.add_argument("--days", type=int, default=14)
    parser.add_argument("--out-dir", default="docs/backtest_reports/bsc_meme_event_replay")
    parser.add_argument("--mode", choices=["price-volume", "full-event"], default="price-volume")
    parser.add_argument("--allow-partial-event-data", action="store_true")
    parser.add_argument("--coinalyze-key-env", default="COINALYZE_API_KEY")
    parser.add_argument("--coinalyze-interval", default="5min")
    parser.add_argument("--etherscan-key-env", default="ETHERSCAN_API_KEY")
    parser.add_argument("--bsc-rpc-env", default="BSC_RPC_URL")
    parser.add_argument("--bsc-rpc-url", default=None)
    parser.add_argument("--chainstack-key-env", default="CHAINSTACK_API_KEY")
    parser.add_argument("--cex-labels-path", default=None)
    parser.add_argument("--discover-cex-candidates", action="store_true")
    parser.add_argument("--cex-candidate-limit", type=int, default=50)
    args = parser.parse_args()
    results, summary = run_online(args)
    write_outputs(Path(args.out_dir), results, summary)
    print(json.dumps(summary, indent=2, ensure_ascii=False))
