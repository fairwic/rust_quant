#!/usr/bin/env python3
import copy
import csv
import json
import os
import random
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any


ROOT = Path("/Users/xu/onions/rust_quant")
REPORT_DIR = ROOT / "docs" / "backtest_reports"
REPORT_DIR.mkdir(parents=True, exist_ok=True)

MYSQL_CMD = [
    "podman",
    "exec",
    "-i",
    "mysql",
    "mysql",
    "-uroot",
    "-pexample",
    "test",
    "-N",
    "-B",
]

RUN_ENV = {
    "TIGHTEN_VEGAS_RISK": "0",
    "DB_HOST": "mysql://root:example@localhost:33306/test?ssl-mode=DISABLED",
}

BASELINE_BACKTEST_ID = 15690
TOTAL_ITERATIONS = 84
PHASE1_COUNT = 48
PHASE2_COUNT = TOTAL_ITERATIONS - PHASE1_COUNT
SEED = 20260312


@dataclass
class Metrics:
    backtest_id: int
    win_rate: float
    profit: float
    sharpe_ratio: float
    max_drawdown: float
    volatility: float
    open_positions_num: int
    created_at: str


def run_cmd(cmd: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    return subprocess.run(
        cmd,
        cwd=ROOT,
        env=merged_env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )


def mysql_query(sql: str) -> str:
    result = run_cmd(MYSQL_CMD + ["-e", sql])
    lines = [
        line
        for line in result.stdout.splitlines()
        if line.strip() and not line.startswith("mysql: [Warning]")
    ]
    return "\n".join(lines).strip()


def mysql_exec(sql: str) -> None:
    run_cmd(MYSQL_CMD + ["-e", sql])


def sql_quote(text: str) -> str:
    return "'" + text.replace("\\", "\\\\").replace("'", "''") + "'"


def fetch_current_config() -> tuple[dict[str, Any], dict[str, Any]]:
    sql = "select value, risk_config from strategy_config where id=11;"
    output = mysql_query(sql)
    value_raw, risk_raw = output.split("\t", 1)
    return json.loads(value_raw), json.loads(risk_raw)


def update_strategy_config(value: dict[str, Any], risk: dict[str, Any]) -> None:
    value_json = json.dumps(value, ensure_ascii=True, separators=(",", ":"))
    risk_json = json.dumps(risk, ensure_ascii=True, separators=(",", ":"))
    sql = (
        "update strategy_config set "
        f"value={sql_quote(value_json)}, risk_config={sql_quote(risk_json)} "
        "where id=11;"
    )
    mysql_exec(sql)


def fetch_metrics(backtest_id: int) -> Metrics:
    sql = (
        "select id, win_rate, profit, sharpe_ratio, max_drawdown, volatility, "
        "open_positions_num, created_at "
        f"from back_test_log where id={backtest_id};"
    )
    output = mysql_query(sql)
    cols = output.split("\t")
    return Metrics(
        backtest_id=int(cols[0]),
        win_rate=float(cols[1]),
        profit=float(cols[2]),
        sharpe_ratio=float(cols[3]),
        max_drawdown=float(cols[4]),
        volatility=float(cols[5]),
        open_positions_num=int(cols[6]),
        created_at=cols[7],
    )


def run_backtest() -> int:
    env = RUN_ENV.copy()
    cmd = [str(ROOT / "target" / "debug" / "rust_quant")]
    proc = subprocess.Popen(
        cmd,
        cwd=ROOT,
        env={**os.environ.copy(), **env},
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    backtest_id = None
    try:
        assert proc.stdout is not None
        for line in proc.stdout:
            match = re.search(r"back_test_id=(\d+)", line)
            if match:
                backtest_id = int(match.group(1))
        proc.wait(timeout=30)
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
    if backtest_id is None:
        raise RuntimeError("failed to parse back_test_id from backtest output")
    return backtest_id


def set_weight(config: dict[str, Any], name: str, value: float) -> None:
    weights = config.setdefault("signal_weights", {}).setdefault("weights", [])
    for item in weights:
        if item[0] == name:
            item[1] = round(value, 4)
            return
    weights.append([name, round(value, 4)])


def get_weight(config: dict[str, Any], name: str, default: float) -> float:
    weights = config.get("signal_weights", {}).get("weights", [])
    for item in weights:
        if item[0] == name:
            return float(item[1])
    return default


def clamp(value: float, low: float, high: float, digits: int = 4) -> float:
    return round(max(low, min(high, value)), digits)


def make_candidate(base_value: dict[str, Any], base_risk: dict[str, Any], params: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    value = copy.deepcopy(base_value)
    risk = copy.deepcopy(base_risk)

    value["rsi_signal"]["rsi_oversold"] = params["rsi_oversold"]
    value["rsi_signal"]["rsi_overbought"] = params["rsi_overbought"]
    value["volume_signal"]["volume_increase_ratio"] = params["volume_increase_ratio"]
    value["signal_weights"]["min_total_weight"] = params["min_total_weight"]
    value["leg_detection_signal"]["size"] = int(params["leg_size"])
    value["range_filter_signal"]["bb_width_threshold"] = params["bb_width_threshold"]
    value["range_filter_signal"]["tp_kline_ratio"] = params["tp_kline_ratio"]
    value["chase_confirm_config"]["long_threshold"] = params["long_threshold"]
    value["chase_confirm_config"]["short_threshold"] = params["short_threshold"]
    value["chase_confirm_config"]["pullback_touch_threshold"] = params["pullback_touch_threshold"]
    value["chase_confirm_config"]["min_body_ratio"] = params["chase_min_body_ratio"]
    value["fib_retracement_signal"]["fib_trigger_low"] = params["fib_trigger_low"]
    value["fib_retracement_signal"]["fib_trigger_high"] = params["fib_trigger_high"]
    value["fib_retracement_signal"]["min_volume_ratio"] = params["fib_min_volume_ratio"]
    value["fib_retracement_signal"]["stop_loss_buffer_ratio"] = params["fib_stop_loss_buffer_ratio"]
    value["extreme_k_filter_signal"]["min_body_ratio"] = params["extreme_min_body_ratio"]
    value["extreme_k_filter_signal"]["min_move_pct"] = params["extreme_min_move_pct"]
    set_weight(value, "LegDetection", params["weight_leg"])
    set_weight(value, "Bolling", params["weight_bolling"])
    set_weight(value, "Engulfing", params["weight_engulfing"])
    set_weight(value, "KlineHammer", params["weight_kline_hammer"])
    set_weight(value, "FairValueGap", params["weight_fvg"])

    risk["max_loss_percent"] = params["max_loss_percent"]
    risk["atr_take_profit_ratio"] = params["atr_take_profit_ratio"]
    risk["fixed_profit_percent_take_profit"] = params["fixed_profit_percent_take_profit"]
    return value, risk


def base_param_state(base_value: dict[str, Any], base_risk: dict[str, Any]) -> dict[str, Any]:
    fib_cfg = base_value["fib_retracement_signal"]
    chase_cfg = base_value["chase_confirm_config"]
    return {
        "rsi_oversold": float(base_value["rsi_signal"]["rsi_oversold"]),
        "rsi_overbought": float(base_value["rsi_signal"]["rsi_overbought"]),
        "volume_increase_ratio": float(base_value["volume_signal"]["volume_increase_ratio"]),
        "min_total_weight": float(base_value["signal_weights"]["min_total_weight"]),
        "leg_size": int(base_value["leg_detection_signal"]["size"]),
        "bb_width_threshold": float(base_value["range_filter_signal"]["bb_width_threshold"]),
        "tp_kline_ratio": float(base_value["range_filter_signal"]["tp_kline_ratio"]),
        "long_threshold": float(chase_cfg["long_threshold"]),
        "short_threshold": float(chase_cfg["short_threshold"]),
        "pullback_touch_threshold": float(chase_cfg["pullback_touch_threshold"]),
        "chase_min_body_ratio": float(chase_cfg["min_body_ratio"]),
        "fib_trigger_low": float(fib_cfg["fib_trigger_low"]),
        "fib_trigger_high": float(fib_cfg["fib_trigger_high"]),
        "fib_min_volume_ratio": float(fib_cfg["min_volume_ratio"]),
        "fib_stop_loss_buffer_ratio": float(fib_cfg["stop_loss_buffer_ratio"]),
        "extreme_min_body_ratio": float(base_value["extreme_k_filter_signal"]["min_body_ratio"]),
        "extreme_min_move_pct": float(base_value["extreme_k_filter_signal"]["min_move_pct"]),
        "weight_leg": get_weight(base_value, "LegDetection", 0.9),
        "weight_bolling": get_weight(base_value, "Bolling", 1.0),
        "weight_engulfing": get_weight(base_value, "Engulfing", 1.0),
        "weight_kline_hammer": get_weight(base_value, "KlineHammer", 1.0),
        "weight_fvg": get_weight(base_value, "FairValueGap", 1.5),
        "max_loss_percent": float(base_risk["max_loss_percent"]),
        "atr_take_profit_ratio": float(base_risk["atr_take_profit_ratio"]),
        "fixed_profit_percent_take_profit": float(base_risk["fixed_profit_percent_take_profit"]),
    }


def sample_phase1(rng: random.Random, baseline: dict[str, Any], index: int) -> dict[str, Any]:
    params = copy.deepcopy(baseline)
    params["rsi_oversold"] = clamp(rng.uniform(10.0, 18.0), 10.0, 18.0, 1)
    params["rsi_overbought"] = clamp(rng.uniform(82.0, 90.0), 82.0, 90.0, 1)
    params["volume_increase_ratio"] = clamp(rng.uniform(2.2, 3.1), 2.2, 3.1, 2)
    params["min_total_weight"] = clamp(rng.uniform(1.85, 2.2), 1.85, 2.2, 2)
    params["leg_size"] = rng.choice([6, 7, 8, 9])
    params["bb_width_threshold"] = clamp(rng.uniform(0.022, 0.036), 0.022, 0.036, 3)
    params["tp_kline_ratio"] = clamp(rng.uniform(0.52, 0.74), 0.52, 0.74, 2)
    params["long_threshold"] = clamp(rng.uniform(0.15, 0.22), 0.15, 0.22, 3)
    params["short_threshold"] = clamp(rng.uniform(0.08, 0.14), 0.08, 0.14, 3)
    params["pullback_touch_threshold"] = clamp(rng.uniform(0.035, 0.06), 0.035, 0.06, 3)
    params["chase_min_body_ratio"] = clamp(rng.uniform(0.42, 0.62), 0.42, 0.62, 2)
    params["fib_trigger_low"] = clamp(rng.uniform(0.29, 0.36), 0.29, 0.36, 3)
    params["fib_trigger_high"] = clamp(rng.uniform(0.58, 0.67), 0.58, 0.67, 3)
    if params["fib_trigger_high"] - params["fib_trigger_low"] < 0.22:
        params["fib_trigger_high"] = clamp(params["fib_trigger_low"] + 0.22, 0.58, 0.67, 3)
    params["fib_min_volume_ratio"] = clamp(rng.uniform(1.7, 2.5), 1.7, 2.5, 2)
    params["fib_stop_loss_buffer_ratio"] = clamp(rng.uniform(0.006, 0.014), 0.006, 0.014, 3)
    params["extreme_min_body_ratio"] = clamp(rng.uniform(0.58, 0.75), 0.58, 0.75, 2)
    params["extreme_min_move_pct"] = clamp(rng.uniform(0.008, 0.016), 0.008, 0.016, 3)
    params["weight_leg"] = clamp(rng.uniform(0.75, 1.05), 0.75, 1.05, 2)
    params["weight_bolling"] = clamp(rng.uniform(0.8, 1.1), 0.8, 1.1, 2)
    params["weight_engulfing"] = clamp(rng.uniform(0.8, 1.1), 0.8, 1.1, 2)
    params["weight_kline_hammer"] = clamp(rng.uniform(0.75, 1.05), 0.75, 1.05, 2)
    params["weight_fvg"] = clamp(rng.uniform(1.2, 1.7), 1.2, 1.7, 2)
    params["max_loss_percent"] = clamp(rng.uniform(0.036, 0.045), 0.036, 0.045, 3)
    params["atr_take_profit_ratio"] = clamp(rng.uniform(2.5, 3.5), 2.5, 3.5, 2)
    params["fixed_profit_percent_take_profit"] = clamp(rng.uniform(0.04, 0.06), 0.04, 0.06, 3)

    if index % 6 == 0:
        params["max_loss_percent"] = clamp(params["max_loss_percent"] - 0.002, 0.036, 0.045, 3)
        params["atr_take_profit_ratio"] = clamp(params["atr_take_profit_ratio"] + 0.15, 2.5, 3.5, 2)
    if index % 6 == 1:
        params["short_threshold"] = clamp(params["short_threshold"] - 0.01, 0.08, 0.14, 3)
        params["fib_min_volume_ratio"] = clamp(params["fib_min_volume_ratio"] + 0.2, 1.7, 2.5, 2)
    if index % 6 == 2:
        params["weight_kline_hammer"] = clamp(params["weight_kline_hammer"] - 0.1, 0.75, 1.05, 2)
        params["weight_engulfing"] = clamp(params["weight_engulfing"] - 0.05, 0.8, 1.1, 2)
    if index % 6 == 3:
        params["tp_kline_ratio"] = clamp(params["tp_kline_ratio"] + 0.08, 0.52, 0.74, 2)
        params["fixed_profit_percent_take_profit"] = clamp(
            params["fixed_profit_percent_take_profit"] + 0.005,
            0.04,
            0.06,
            3,
        )
    if index % 6 == 4:
        params["rsi_oversold"] = clamp(params["rsi_oversold"] + 2.0, 10.0, 18.0, 1)
        params["bb_width_threshold"] = clamp(params["bb_width_threshold"] - 0.004, 0.022, 0.036, 3)
    return params


def sample_phase2(rng: random.Random, parent: dict[str, Any]) -> dict[str, Any]:
    params = copy.deepcopy(parent)
    params["rsi_oversold"] = clamp(parent["rsi_oversold"] + rng.uniform(-1.5, 1.5), 10.0, 18.0, 1)
    params["rsi_overbought"] = clamp(parent["rsi_overbought"] + rng.uniform(-2.0, 2.0), 82.0, 90.0, 1)
    params["volume_increase_ratio"] = clamp(parent["volume_increase_ratio"] + rng.uniform(-0.15, 0.15), 2.2, 3.1, 2)
    params["min_total_weight"] = clamp(parent["min_total_weight"] + rng.uniform(-0.08, 0.08), 1.85, 2.2, 2)
    params["leg_size"] = int(min(9, max(6, parent["leg_size"] + rng.choice([-1, 0, 1]))))
    params["bb_width_threshold"] = clamp(parent["bb_width_threshold"] + rng.uniform(-0.003, 0.003), 0.022, 0.036, 3)
    params["tp_kline_ratio"] = clamp(parent["tp_kline_ratio"] + rng.uniform(-0.05, 0.05), 0.52, 0.74, 2)
    params["long_threshold"] = clamp(parent["long_threshold"] + rng.uniform(-0.015, 0.015), 0.15, 0.22, 3)
    params["short_threshold"] = clamp(parent["short_threshold"] + rng.uniform(-0.015, 0.015), 0.08, 0.14, 3)
    params["pullback_touch_threshold"] = clamp(parent["pullback_touch_threshold"] + rng.uniform(-0.006, 0.006), 0.035, 0.06, 3)
    params["chase_min_body_ratio"] = clamp(parent["chase_min_body_ratio"] + rng.uniform(-0.05, 0.05), 0.42, 0.62, 2)
    params["fib_trigger_low"] = clamp(parent["fib_trigger_low"] + rng.uniform(-0.02, 0.02), 0.29, 0.36, 3)
    params["fib_trigger_high"] = clamp(parent["fib_trigger_high"] + rng.uniform(-0.02, 0.02), 0.58, 0.67, 3)
    if params["fib_trigger_high"] - params["fib_trigger_low"] < 0.22:
        params["fib_trigger_high"] = clamp(params["fib_trigger_low"] + 0.22, 0.58, 0.67, 3)
    params["fib_min_volume_ratio"] = clamp(parent["fib_min_volume_ratio"] + rng.uniform(-0.15, 0.15), 1.7, 2.5, 2)
    params["fib_stop_loss_buffer_ratio"] = clamp(parent["fib_stop_loss_buffer_ratio"] + rng.uniform(-0.002, 0.002), 0.006, 0.014, 3)
    params["extreme_min_body_ratio"] = clamp(parent["extreme_min_body_ratio"] + rng.uniform(-0.04, 0.04), 0.58, 0.75, 2)
    params["extreme_min_move_pct"] = clamp(parent["extreme_min_move_pct"] + rng.uniform(-0.002, 0.002), 0.008, 0.016, 3)
    params["weight_leg"] = clamp(parent["weight_leg"] + rng.uniform(-0.08, 0.08), 0.75, 1.05, 2)
    params["weight_bolling"] = clamp(parent["weight_bolling"] + rng.uniform(-0.08, 0.08), 0.8, 1.1, 2)
    params["weight_engulfing"] = clamp(parent["weight_engulfing"] + rng.uniform(-0.08, 0.08), 0.8, 1.1, 2)
    params["weight_kline_hammer"] = clamp(parent["weight_kline_hammer"] + rng.uniform(-0.08, 0.08), 0.75, 1.05, 2)
    params["weight_fvg"] = clamp(parent["weight_fvg"] + rng.uniform(-0.1, 0.1), 1.2, 1.7, 2)
    params["max_loss_percent"] = clamp(parent["max_loss_percent"] + rng.uniform(-0.002, 0.002), 0.036, 0.045, 3)
    params["atr_take_profit_ratio"] = clamp(parent["atr_take_profit_ratio"] + rng.uniform(-0.2, 0.2), 2.5, 3.5, 2)
    params["fixed_profit_percent_take_profit"] = clamp(parent["fixed_profit_percent_take_profit"] + rng.uniform(-0.004, 0.004), 0.04, 0.06, 3)
    return params


def score(metrics: Metrics, baseline: Metrics) -> float:
    score_value = 0.0
    score_value += (metrics.profit - baseline.profit)
    score_value += (metrics.win_rate - baseline.win_rate) * 40000.0
    score_value += (baseline.max_drawdown - metrics.max_drawdown) * 8000.0
    score_value += (metrics.sharpe_ratio - baseline.sharpe_ratio) * 1500.0
    if (
        metrics.profit > baseline.profit
        and metrics.win_rate > baseline.win_rate
        and metrics.max_drawdown < baseline.max_drawdown
    ):
        score_value += 100000.0
    return round(score_value, 4)


def flatten_params(params: dict[str, Any]) -> dict[str, Any]:
    return {key: params[key] for key in sorted(params)}


def write_iteration(writer: csv.DictWriter, row: dict[str, Any], handle) -> None:
    writer.writerow(row)
    handle.flush()


def main() -> int:
    rng = random.Random(SEED)
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    report_tsv = REPORT_DIR / f"vegas_opt_batch_{timestamp}.tsv"
    report_jsonl = REPORT_DIR / f"vegas_opt_batch_{timestamp}.jsonl"
    summary_json = REPORT_DIR / f"vegas_opt_batch_{timestamp}_summary.json"

    base_value, base_risk = fetch_current_config()
    original_value = copy.deepcopy(base_value)
    original_risk = copy.deepcopy(base_risk)
    baseline_metrics = fetch_metrics(BASELINE_BACKTEST_ID)
    baseline_params = base_param_state(base_value, base_risk)

    results: list[dict[str, Any]] = []
    top_candidates: list[dict[str, Any]] = []
    best_result: dict[str, Any] | None = None
    strict_best: dict[str, Any] | None = None

    fieldnames = [
        "iteration",
        "phase",
        "backtest_id",
        "profit",
        "win_rate",
        "sharpe_ratio",
        "max_drawdown",
        "volatility",
        "open_positions_num",
        "score",
        "beats_profit",
        "beats_win_rate",
        "beats_drawdown",
        "strict_dominator",
        "params_json",
    ]

    try:
        with report_tsv.open("w", newline="", encoding="utf-8") as tsv_handle, report_jsonl.open(
            "w", encoding="utf-8"
        ) as jsonl_handle:
            writer = csv.DictWriter(tsv_handle, fieldnames=fieldnames, delimiter="\t")
            writer.writeheader()
            tsv_handle.flush()

            for idx in range(TOTAL_ITERATIONS):
                phase = 1 if idx < PHASE1_COUNT else 2
                if phase == 1:
                    params = sample_phase1(rng, baseline_params, idx)
                else:
                    if not top_candidates:
                        top_candidates = sorted(results, key=lambda x: x["score"], reverse=True)[:6]
                    parent = rng.choice(top_candidates)["params"]
                    params = sample_phase2(rng, parent)

                value, risk = make_candidate(base_value, base_risk, params)
                update_strategy_config(value, risk)
                started_at = time.time()
                backtest_id = run_backtest()
                metrics = fetch_metrics(backtest_id)
                elapsed = round(time.time() - started_at, 3)
                row = {
                    "iteration": idx + 1,
                    "phase": phase,
                    "backtest_id": metrics.backtest_id,
                    "profit": round(metrics.profit, 2),
                    "win_rate": round(metrics.win_rate, 6),
                    "sharpe_ratio": round(metrics.sharpe_ratio, 5),
                    "max_drawdown": round(metrics.max_drawdown, 6),
                    "volatility": round(metrics.volatility, 6),
                    "open_positions_num": metrics.open_positions_num,
                    "score": score(metrics, baseline_metrics),
                    "beats_profit": metrics.profit > baseline_metrics.profit,
                    "beats_win_rate": metrics.win_rate > baseline_metrics.win_rate,
                    "beats_drawdown": metrics.max_drawdown < baseline_metrics.max_drawdown,
                    "strict_dominator": (
                        metrics.profit > baseline_metrics.profit
                        and metrics.win_rate > baseline_metrics.win_rate
                        and metrics.max_drawdown < baseline_metrics.max_drawdown
                    ),
                    "params_json": json.dumps(flatten_params(params), ensure_ascii=True, separators=(",", ":")),
                }
                row["elapsed_sec"] = elapsed
                row["params"] = params
                results.append(row)
                if best_result is None or row["score"] > best_result["score"]:
                    best_result = row
                if row["strict_dominator"]:
                    if strict_best is None or row["score"] > strict_best["score"]:
                        strict_best = row

                if phase == 1:
                    top_candidates = sorted(results, key=lambda x: x["score"], reverse=True)[:6]
                else:
                    merged = top_candidates + [row]
                    top_candidates = sorted(merged, key=lambda x: x["score"], reverse=True)[:6]

                write_iteration(
                    writer,
                    {key: row[key] for key in fieldnames},
                    tsv_handle,
                )
                jsonl_handle.write(
                    json.dumps(
                        {
                            key: row[key]
                            for key in (
                                "iteration",
                                "phase",
                                "backtest_id",
                                "profit",
                                "win_rate",
                                "sharpe_ratio",
                                "max_drawdown",
                                "volatility",
                                "open_positions_num",
                                "score",
                                "beats_profit",
                                "beats_win_rate",
                                "beats_drawdown",
                                "strict_dominator",
                                "elapsed_sec",
                            )
                        }
                        | {"params": flatten_params(params)},
                        ensure_ascii=True,
                    )
                    + "\n"
                )
                jsonl_handle.flush()
                print(
                    f"[{idx + 1:02d}/{TOTAL_ITERATIONS}] id={metrics.backtest_id} "
                    f"profit={metrics.profit:.2f} win={metrics.win_rate:.4f} "
                    f"sharpe={metrics.sharpe_ratio:.4f} dd={metrics.max_drawdown:.4f} "
                    f"score={row['score']:.2f} strict={row['strict_dominator']} "
                    f"elapsed={elapsed:.2f}s",
                    flush=True,
                )
    finally:
        chosen = strict_best or best_result
        if chosen is not None:
            best_value, best_risk = make_candidate(base_value, base_risk, chosen["params"])
            update_strategy_config(best_value, best_risk)
        else:
            update_strategy_config(original_value, original_risk)

    strict_dominators = [item for item in results if item["strict_dominator"]]
    summary = {
        "seed": SEED,
        "baseline_backtest_id": BASELINE_BACKTEST_ID,
        "baseline_metrics": baseline_metrics.__dict__,
        "total_iterations": TOTAL_ITERATIONS,
        "strict_dominator_count": len(strict_dominators),
        "best_result": {
            key: best_result[key]
            for key in (
                "iteration",
                "phase",
                "backtest_id",
                "profit",
                "win_rate",
                "sharpe_ratio",
                "max_drawdown",
                "volatility",
                "open_positions_num",
                "score",
                "strict_dominator",
            )
        }
        if best_result
        else None,
        "strict_best_result": {
            key: strict_best[key]
            for key in (
                "iteration",
                "phase",
                "backtest_id",
                "profit",
                "win_rate",
                "sharpe_ratio",
                "max_drawdown",
                "volatility",
                "open_positions_num",
                "score",
                "strict_dominator",
            )
        }
        if strict_best
        else None,
        "report_tsv": str(report_tsv),
        "report_jsonl": str(report_jsonl),
        "top_10": [
            {
                key: item[key]
                for key in (
                    "iteration",
                    "phase",
                    "backtest_id",
                    "profit",
                    "win_rate",
                    "sharpe_ratio",
                    "max_drawdown",
                    "score",
                    "strict_dominator",
                )
            }
            for item in sorted(results, key=lambda x: x["score"], reverse=True)[:10]
        ],
    }
    summary_json.write_text(json.dumps(summary, ensure_ascii=True, indent=2), encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=True, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
