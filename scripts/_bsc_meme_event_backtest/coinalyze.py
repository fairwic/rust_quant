from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

COINALYZE_BASE = "https://api.coinalyze.net/v1"
_FUTURE_MARKETS_CACHE: list[dict[str, Any]] | None = None


def fetch_coinalyze_history(
    sample: dict[str, Any],
    api_key: str | None,
    start_ts: int,
    end_ts: int,
    interval: str = "5min",
) -> dict[str, Any]:
    if not api_key:
        return {"available": False, "error": "COINALYZE_API_KEY_MISSING"}

    try:
        markets = coinalyze_future_markets(api_key)
    except urllib.error.HTTPError as exc:
        return {"available": False, "error": f"COINALYZE_HTTP_{exc.code}"}
    except Exception:
        return {"available": False, "error": "COINALYZE_REQUEST_FAILED"}
    market = pick_coinalyze_market(sample, markets)
    if not market:
        return {"available": False, "error": "COINALYZE_MARKET_NOT_FOUND", "markets": markets}

    symbol = market["symbol"]
    params = {"symbols": symbol, "interval": interval, "from": start_ts, "to": end_ts}
    try:
        oi = coinalyze_get("/open-interest-history", api_key, {**params, "convert_to_usd": "true"})
        funding = coinalyze_get("/funding-rate-history", api_key, params)
    except urllib.error.HTTPError as exc:
        return {"available": False, "error": f"COINALYZE_HTTP_{exc.code}", "symbol": symbol}
    except Exception:
        return {"available": False, "error": "COINALYZE_REQUEST_FAILED", "symbol": symbol}
    long_short = []
    if market.get("has_long_short_ratio_data"):
        try:
            long_short = coinalyze_get("/long-short-ratio-history", api_key, params)
        except Exception:
            long_short = []
    return build_coinalyze_summary(symbol, market, oi, funding, long_short)


def pick_coinalyze_market(
    sample: dict[str, Any], markets: list[dict[str, Any]]
) -> dict[str, Any] | None:
    base = sample["symbol"].split("_", 1)[0].upper()
    aliases = {_normalize_symbol(s) for s in sample.get("perp_symbols", [])}
    aliases.add(f"{base}USDT")
    aliases.add(f"{base}USD")

    candidates = []
    for market in markets:
        if str(market.get("base_asset", "")).upper() != base:
            continue
        if str(market.get("quote_asset", "")).upper() not in {"USDT", "USD"}:
            continue
        if not market.get("is_perpetual"):
            continue
        score = _market_score(market, aliases)
        candidates.append((score, market))
    if not candidates:
        return None
    candidates.sort(key=lambda item: item[0], reverse=True)
    return candidates[0][1]


def build_coinalyze_summary(
    symbol: str,
    market: dict[str, Any],
    oi_payload: list[dict[str, Any]],
    funding_payload: list[dict[str, Any]],
    long_short_payload: list[dict[str, Any]],
) -> dict[str, Any]:
    oi_history = _history_for_symbol(symbol, oi_payload)
    funding_history = _history_for_symbol(symbol, funding_payload)
    ratio_history = _history_for_symbol(symbol, long_short_payload)
    result = {
        "available": bool(oi_history),
        "symbol": symbol,
        "market": market,
        "oi_growth_1h_pct": max_growth_pct(oi_history, 12),
        "oi_growth_4h_pct": max_growth_pct(oi_history, 48),
        "funding_rate": min_close(funding_history),
        "short_crowding_score": max_short_crowding(ratio_history),
        "raw_open_interest": oi_payload,
        "raw_funding": funding_payload,
        "raw_long_short": long_short_payload,
    }
    if not oi_history:
        result["error"] = "COINALYZE_OI_HISTORY_EMPTY"
    return result


def coinalyze_get(path: str, api_key: str, params: dict[str, Any]) -> Any:
    query = urllib.parse.urlencode({**params, "api_key": api_key})
    url = f"{COINALYZE_BASE}{path}?{query}" if query else f"{COINALYZE_BASE}{path}"
    req = urllib.request.Request(url, headers={"User-Agent": "rust-quant-backtest/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode("utf-8"))


def coinalyze_future_markets(api_key: str) -> list[dict[str, Any]]:
    global _FUTURE_MARKETS_CACHE
    if _FUTURE_MARKETS_CACHE is None:
        _FUTURE_MARKETS_CACHE = coinalyze_get("/future-markets", api_key, {})
    return _FUTURE_MARKETS_CACHE


def max_growth_pct(history: list[dict[str, Any]], lookback_bars: int) -> float | None:
    if len(history) <= lookback_bars:
        return None
    best = None
    for i in range(lookback_bars, len(history)):
        prev = _float(history[i - lookback_bars].get("c"))
        current = _float(history[i].get("c"))
        if not prev or current is None:
            continue
        growth = (current / prev - 1.0) * 100.0
        best = growth if best is None else max(best, growth)
    return best


def min_close(history: list[dict[str, Any]]) -> float | None:
    values = [_float(item.get("c")) for item in history]
    values = [value for value in values if value is not None]
    return min(values) if values else None


def max_short_crowding(history: list[dict[str, Any]]) -> float | None:
    scores = []
    for item in history:
        long_value = _float(item.get("l"))
        short_value = _float(item.get("s"))
        if long_value is None or short_value is None:
            continue
        total = long_value + short_value
        if total > 0:
            scores.append(short_value / total)
    return max(scores) if scores else None


def _history_for_symbol(symbol: str, payload: list[dict[str, Any]]) -> list[dict[str, Any]]:
    for item in payload or []:
        if item.get("symbol") == symbol:
            return item.get("history") or []
    return []


def _market_score(market: dict[str, Any], aliases: set[str]) -> int:
    symbol_on_exchange = _normalize_symbol(str(market.get("symbol_on_exchange", "")))
    symbol = _normalize_symbol(str(market.get("symbol", "")))
    score = 0
    if symbol_on_exchange in aliases:
        score += 100
    if any(alias in symbol for alias in aliases):
        score += 50
    if market.get("has_long_short_ratio_data"):
        score += 10
    if market.get("has_ohlcv_data"):
        score += 5
    return score


def _normalize_symbol(value: str) -> str:
    return value.upper().replace("_", "").replace("-", "")


def _float(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None
