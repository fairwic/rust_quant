#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping


SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from postgres_backtest import query_rows, query_scalar, quote_identifier  # noqa: E402


ENTRY_OPTIONS = {"long", "short"}
NEAR_POC_PCT = 0.005
MID_POC_PCT = 0.02


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Analyze Vegas trade PnL buckets from signal_value.volume_profile_value."
    )
    parser.add_argument(
        "--back-test-id",
        type=int,
        help="Backtest id to analyze. Defaults to the latest Vegas backtest.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=5000,
        help="Maximum back_test_detail rows to load.",
    )
    parser.add_argument(
        "--min-samples",
        type=int,
        default=5,
        help="Minimum bucket sample count before it appears as positive/negative candidate.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print the full report as JSON.",
    )
    parser.add_argument(
        "--backfill-from-candles",
        action="store_true",
        help="Recalculate volume profile from candle rows when signal_value lacks it.",
    )
    parser.add_argument("--lookback", type=int, default=48, help="Volume profile candle lookback.")
    parser.add_argument("--price-bins", type=int, default=24, help="Volume profile price bins.")
    parser.add_argument(
        "--value-area-ratio",
        type=float,
        default=0.70,
        help="Volume profile value area ratio.",
    )
    return parser.parse_args()


def latest_vegas_back_test_id() -> int:
    return int(query_scalar(latest_vegas_back_test_sql()))


def latest_vegas_back_test_sql() -> str:
    return (
        "SELECT id FROM back_test_log "
        "WHERE lower(strategy_type) = 'vegas' "
        "ORDER BY id DESC LIMIT 1"
    )


def fetch_detail_rows(back_test_id: int, limit: int) -> list[Mapping[str, Any]]:
    return query_rows(
        "SELECT id, back_test_id, option_type, profit_loss, signal_value, "
        "signal_open_position_time, open_position_time, close_position_time "
        "FROM back_test_detail "
        f"WHERE back_test_id = {int(back_test_id)} "
        "ORDER BY open_position_time ASC, id ASC "
        f"LIMIT {int(limit)}"
    )


def fetch_candle_rows_for_backtest(back_test_id: int) -> list[Mapping[str, Any]]:
    rows = query_rows(
        "SELECT inst_type, time, kline_start_time, kline_end_time "
        f"FROM back_test_log WHERE id = {int(back_test_id)} LIMIT 1"
    )
    if not rows:
        raise RuntimeError(f"back_test_log not found: {back_test_id}")

    log = rows[0]
    table_name = quote_identifier(f"{str(log['inst_type']).lower()}_candles_{str(log['time']).lower()}")
    return query_rows(
        "SELECT ts, o, h, l, c, vol "
        f"FROM {table_name} "
        f"WHERE ts >= {int(log['kline_start_time'])} "
        f"AND ts <= {int(log['kline_end_time'])} "
        "ORDER BY ts ASC"
    )


def analyze_rows(
    rows: list[Mapping[str, Any]],
    min_samples: int = 5,
    profile_by_open_time: Mapping[str, Mapping[str, Any]] | None = None,
) -> dict[str, Any]:
    buckets: dict[str, dict[str, float | int]] = {}
    analyzed_trades = 0
    backfilled_profile_trades = 0
    missing_profile_trades = 0
    paired_trades, skipped_rows, missing_close_trades = pair_entry_close_rows(rows)

    for entry_row, close_row in paired_trades:
        direction = str(entry_row.get("option_type") or "").lower()
        profile = extract_volume_profile(entry_row.get("signal_value"))
        if profile is None and profile_by_open_time is not None:
            profile = profile_by_open_time.get(normalize_time_key(entry_row.get("open_position_time")))
            if profile is not None:
                backfilled_profile_trades += 1
        if profile is None:
            missing_profile_trades += 1
            continue

        profit = float_or_zero(close_row.get("profit_loss"))
        analyzed_trades += 1

        position = position_bucket(profile)
        node = node_bucket(profile)
        poc_distance = poc_distance_bucket(profile)
        for key in (
            f"position:{position}",
            f"direction_position:{direction}_{position}",
            f"node:{node}",
            f"direction_node:{direction}_{node}",
            f"poc_distance:{poc_distance}",
            f"direction_poc_distance:{direction}_{poc_distance}",
            f"position_node:{position}_{node}",
            f"direction_position_node:{direction}_{position}_{node}",
        ):
            add_profit(buckets, key, profit)

    finalized = {key: finalize_stats(value) for key, value in sorted(buckets.items())}
    candidates = candidate_buckets(finalized, min_samples)
    return {
        "summary": {
            "analyzed_trades": analyzed_trades,
            "paired_trades": len(paired_trades),
            "backfilled_profile_trades": backfilled_profile_trades,
            "skipped_rows": skipped_rows,
            "missing_close_trades": missing_close_trades,
            "missing_profile_trades": missing_profile_trades,
            "bucket_count": len(finalized),
        },
        "buckets": finalized,
        "negative_buckets": candidates["negative"],
        "positive_buckets": candidates["positive"],
    }


def build_profile_lookup(
    candle_rows: list[Mapping[str, Any]],
    lookback: int = 48,
    price_bins: int = 24,
    value_area_ratio: float = 0.70,
) -> dict[str, Mapping[str, Any]]:
    window: list[dict[str, float]] = []
    lookup: dict[str, Mapping[str, Any]] = {}
    for row in sorted(candle_rows, key=lambda item: int(item["ts"])):
        candle = {
            "ts": int(row["ts"]),
            "h": float_or_zero(row.get("h")),
            "l": float_or_zero(row.get("l")),
            "c": float_or_zero(row.get("c")),
            "vol": float_or_zero(row.get("vol")),
        }
        window.append(candle)
        while len(window) > max(1, lookback):
            window.pop(0)
        lookup[timestamp_ms_to_time_key(candle["ts"])] = calculate_volume_profile(
            window,
            price_bins=max(1, min(200, price_bins)),
            value_area_ratio=max(0.0, min(1.0, value_area_ratio)),
            close=candle["c"],
        )
    return lookup


def calculate_volume_profile(
    candles: list[Mapping[str, float]],
    price_bins: int,
    value_area_ratio: float,
    close: float,
) -> Mapping[str, Any]:
    valid_candles = [
        candle
        for candle in candles
        if candle["vol"] > 0.0 and candle["h"] >= candle["l"] and candle["h"] > 0.0
    ]
    if not valid_candles:
        return empty_profile(close, price_bins)

    min_price = min(min(candle["l"], candle["h"]) for candle in valid_candles)
    max_price = max(max(candle["l"], candle["h"]) for candle in valid_candles)
    if max_price <= min_price:
        return empty_profile(close, price_bins)

    bin_width = (max_price - min_price) / price_bins
    if bin_width <= 0.0:
        return empty_profile(close, price_bins)

    volumes = [0.0] * price_bins
    for candle in valid_candles:
        distribute_candle_volume(candle, min_price, max_price, bin_width, volumes)

    total_volume = sum(volumes)
    if total_volume <= 0.0:
        return empty_profile(close, price_bins)

    poc_index = max(range(len(volumes)), key=lambda index: volumes[index])
    value_low_index, value_high_index = value_area_indexes(
        volumes,
        poc_index,
        total_volume * value_area_ratio,
    )
    value_area_low = bin_low(min_price, bin_width, value_low_index)
    value_area_high = bin_high(min_price, max_price, bin_width, value_high_index, price_bins)
    point_of_control = (
        bin_low(min_price, bin_width, poc_index)
        + bin_high(min_price, max_price, bin_width, poc_index, price_bins)
    ) / 2.0
    close_index = price_to_bin(close, min_price, max_price, bin_width, price_bins)
    close_bin_volume = volumes[close_index]
    average_bin_volume = total_volume / price_bins

    return {
        "point_of_control": point_of_control,
        "value_area_high": value_area_high,
        "value_area_low": value_area_low,
        "total_volume": round(total_volume, 6),
        "price_bin_count": price_bins,
        "close_bin_volume_ratio": close_bin_volume / total_volume,
        "distance_to_poc_pct": (close - point_of_control) / point_of_control
        if point_of_control > 0.0
        else 0.0,
        "close_above_value_area": close > value_area_high,
        "close_below_value_area": close < value_area_low,
        "close_inside_value_area": value_area_low <= close <= value_area_high,
        "close_on_high_volume_node": close_bin_volume >= average_bin_volume * 1.25,
        "close_on_low_volume_node": close_bin_volume <= average_bin_volume * 0.75,
    }


def distribute_candle_volume(
    candle: Mapping[str, float],
    min_price: float,
    max_price: float,
    bin_width: float,
    volumes: list[float],
) -> None:
    low = min(candle["l"], candle["h"])
    high = max(candle["l"], candle["h"])
    candle_range = high - low
    if candle_range <= 0.0:
        volumes[price_to_bin(candle["c"], min_price, max_price, bin_width, len(volumes))] += candle[
            "vol"
        ]
        return

    for index in range(len(volumes)):
        current_low = bin_low(min_price, bin_width, index)
        current_high = bin_high(min_price, max_price, bin_width, index, len(volumes))
        overlap = min(high, current_high) - max(low, current_low)
        if overlap > 0.0:
            volumes[index] += candle["vol"] * (overlap / candle_range)


def value_area_indexes(volumes: list[float], poc_index: int, target_volume: float) -> tuple[int, int]:
    low_index = poc_index
    high_index = poc_index
    accumulated = volumes[poc_index]
    while accumulated < target_volume and (low_index > 0 or high_index + 1 < len(volumes)):
        left_volume = volumes[low_index - 1] if low_index > 0 else None
        right_volume = volumes[high_index + 1] if high_index + 1 < len(volumes) else None
        if left_volume is not None and right_volume is not None:
            if left_volume > right_volume:
                low_index -= 1
                accumulated += left_volume
            else:
                high_index += 1
                accumulated += right_volume
        elif left_volume is not None:
            low_index -= 1
            accumulated += left_volume
        elif right_volume is not None:
            high_index += 1
            accumulated += right_volume
        else:
            break
    return low_index, high_index


def price_to_bin(
    price: float,
    min_price: float,
    max_price: float,
    bin_width: float,
    price_bins: int,
) -> int:
    if price <= min_price:
        return 0
    if price >= max_price:
        return price_bins - 1
    return int((price - min_price) // bin_width)


def bin_low(min_price: float, bin_width: float, index: int) -> float:
    return min_price + bin_width * index


def bin_high(
    min_price: float,
    max_price: float,
    bin_width: float,
    index: int,
    price_bins: int,
) -> float:
    if index + 1 == price_bins:
        return max_price
    return min_price + bin_width * (index + 1)


def empty_profile(close: float, price_bins: int) -> Mapping[str, Any]:
    return {
        "point_of_control": close,
        "value_area_high": close,
        "value_area_low": close,
        "total_volume": 0.0,
        "price_bin_count": price_bins,
        "close_bin_volume_ratio": 0.0,
        "distance_to_poc_pct": 0.0,
        "close_above_value_area": False,
        "close_below_value_area": False,
        "close_inside_value_area": True,
        "close_on_high_volume_node": False,
        "close_on_low_volume_node": False,
    }


def timestamp_ms_to_time_key(ts: int) -> str:
    return datetime.fromtimestamp(ts / 1000, tz=timezone.utc).strftime("%Y-%m-%d %H:%M:%S")


def pair_entry_close_rows(
    rows: list[Mapping[str, Any]]
) -> tuple[list[tuple[Mapping[str, Any], Mapping[str, Any]]], int, int]:
    pending_entries: dict[str, list[Mapping[str, Any]]] = {}
    paired_trades: list[tuple[Mapping[str, Any], Mapping[str, Any]]] = []
    skipped_rows = 0

    for row in rows:
        option_type = str(row.get("option_type") or "").lower()
        open_time = normalize_time_key(row.get("open_position_time"))
        if option_type in ENTRY_OPTIONS:
            pending_entries.setdefault(open_time, []).append(row)
            continue

        if option_type == "close":
            candidates = pending_entries.get(open_time)
            if candidates:
                entry_row = candidates.pop(0)
                if not candidates:
                    pending_entries.pop(open_time, None)
                paired_trades.append((entry_row, row))
            else:
                skipped_rows += 1
            continue

        skipped_rows += 1

    missing_close_trades = sum(len(value) for value in pending_entries.values())
    return paired_trades, skipped_rows, missing_close_trades


def extract_volume_profile(raw: Any) -> Mapping[str, Any] | None:
    if raw is None:
        return None
    if isinstance(raw, Mapping):
        payload = raw
    else:
        try:
            payload = json.loads(str(raw))
        except (TypeError, json.JSONDecodeError):
            return None

    if not isinstance(payload, Mapping):
        return None
    profile = payload.get("volume_profile_value")
    if isinstance(profile, Mapping):
        return profile
    if "point_of_control" in payload:
        return payload
    return None


def normalize_time_key(value: Any) -> str:
    return "" if value is None else str(value)


def position_bucket(profile: Mapping[str, Any]) -> str:
    if is_truthy(profile.get("close_above_value_area")):
        return "above_value_area"
    if is_truthy(profile.get("close_below_value_area")):
        return "below_value_area"
    if is_truthy(profile.get("close_inside_value_area")):
        return "inside_value_area"
    return "unknown_value_area"


def node_bucket(profile: Mapping[str, Any]) -> str:
    high = is_truthy(profile.get("close_on_high_volume_node"))
    low = is_truthy(profile.get("close_on_low_volume_node"))
    if high and low:
        return "mixed_volume_node"
    if high:
        return "high_volume_node"
    if low:
        return "low_volume_node"
    return "normal_volume_node"


def poc_distance_bucket(profile: Mapping[str, Any]) -> str:
    distance = abs(float_or_zero(profile.get("distance_to_poc_pct")))
    if distance <= NEAR_POC_PCT:
        return "near_poc"
    if distance <= MID_POC_PCT:
        return "mid_poc"
    return "far_poc"


def add_profit(buckets: dict[str, dict[str, float | int]], key: str, profit: float) -> None:
    stats = buckets.setdefault(
        key,
        {
            "count": 0,
            "wins": 0,
            "losses": 0,
            "breakeven": 0,
            "total_profit": 0.0,
        },
    )
    stats["count"] = int(stats["count"]) + 1
    stats["total_profit"] = float(stats["total_profit"]) + profit
    if profit > 0:
        stats["wins"] = int(stats["wins"]) + 1
    elif profit < 0:
        stats["losses"] = int(stats["losses"]) + 1
    else:
        stats["breakeven"] = int(stats["breakeven"]) + 1


def finalize_stats(stats: Mapping[str, float | int]) -> dict[str, float | int]:
    count = int(stats["count"])
    wins = int(stats["wins"])
    total_profit = round(float(stats["total_profit"]), 6)
    return {
        "count": count,
        "wins": wins,
        "losses": int(stats["losses"]),
        "breakeven": int(stats["breakeven"]),
        "total_profit": total_profit,
        "avg_profit": round(total_profit / count, 6) if count else 0.0,
        "win_rate": round(wins / count, 6) if count else 0.0,
    }


def candidate_buckets(
    buckets: Mapping[str, Mapping[str, float | int]], min_samples: int
) -> dict[str, list[dict[str, float | int | str]]]:
    eligible = [
        {"bucket": key, **value}
        for key, value in buckets.items()
        if int(value["count"]) >= min_samples
    ]
    negative = [item for item in eligible if float(item["total_profit"]) < 0]
    positive = [item for item in eligible if float(item["total_profit"]) > 0]
    negative.sort(key=lambda item: (float(item["total_profit"]), -int(item["count"])))
    positive.sort(key=lambda item: (-float(item["total_profit"]), -int(item["count"])))
    return {"negative": negative, "positive": positive}


def is_truthy(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes"}
    return bool(value)


def float_or_zero(value: Any) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


def print_text_report(back_test_id: int, report: Mapping[str, Any]) -> None:
    summary = report["summary"]
    print(f"=== Volume Profile Bucket Analysis: back_test_id={back_test_id} ===")
    print(
        "trades="
        f"{summary['analyzed_trades']} paired={summary['paired_trades']} "
        f"backfilled={summary['backfilled_profile_trades']} "
        f"missing_profile={summary['missing_profile_trades']} "
        f"missing_close={summary['missing_close_trades']} skipped_rows={summary['skipped_rows']} "
        f"buckets={summary['bucket_count']}"
    )
    print_bucket_section("Negative candidates", report["negative_buckets"])
    print_bucket_section("Positive candidates", report["positive_buckets"])


def print_bucket_section(title: str, rows: list[Mapping[str, Any]]) -> None:
    print(f"\n{title}:")
    if not rows:
        print("  none")
        return
    for row in rows[:20]:
        print(
            f"  {row['bucket']:<60} count={row['count']:<4} "
            f"win_rate={row['win_rate']:<8} total={row['total_profit']:<12} "
            f"avg={row['avg_profit']}"
        )


def main() -> None:
    args = parse_args()
    back_test_id = args.back_test_id or latest_vegas_back_test_id()
    rows = fetch_detail_rows(back_test_id, args.limit)
    profile_by_open_time = None
    if args.backfill_from_candles:
        profile_by_open_time = build_profile_lookup(
            fetch_candle_rows_for_backtest(back_test_id),
            lookback=args.lookback,
            price_bins=args.price_bins,
            value_area_ratio=args.value_area_ratio,
        )
    report = analyze_rows(rows, args.min_samples, profile_by_open_time=profile_by_open_time)
    if args.json:
        print(json.dumps({"back_test_id": back_test_id, **report}, ensure_ascii=False, indent=2))
    else:
        print_text_report(back_test_id, report)


if __name__ == "__main__":
    main()
