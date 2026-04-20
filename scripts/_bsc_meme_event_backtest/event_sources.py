from __future__ import annotations

import json
import subprocess
import urllib.parse
import urllib.request
from typing import Any

from .cex_flow import fetch_cex_flow
from .coinalyze import fetch_coinalyze_history
from .event import (
    EventSnapshot,
    merge_cex_flow,
    merge_coinalyze_history,
    merge_derivatives,
    parse_goplus_security,
)

GOPLUS_URL = "https://api.gopluslabs.io/api/v1/token_security/56"
BITGET_BASE = "https://api.bitget.com/api/v2/mix/market"
GATE_BASE = "https://api.gateio.ws/api/v4/futures/usdt"


def collect_event_snapshot(
    sample: dict[str, Any],
    start_ts: int | None = None,
    end_ts: int | None = None,
    coinalyze_key: str | None = None,
    coinalyze_interval: str = "5min",
    etherscan_key: str | None = None,
    cex_labels_path: str | None = None,
    reference_price_usd: float = 0.0,
    bsc_rpc_url: str | None = None,
) -> tuple[EventSnapshot, dict[str, Any]]:
    symbol = sample["symbol"]
    contract = sample.get("contract_address")
    event = EventSnapshot(
        symbol=symbol,
        contract_address=contract,
        event_tags=list(sample.get("event_tags", [])),
    )
    raw: dict[str, Any] = {}

    if contract:
        security = fetch_goplus_security(contract)
        raw["goplus_security"] = security
        event = parse_goplus_security(symbol, contract, security)
        event.event_tags = list(sample.get("event_tags", []))
    else:
        event.warnings.append("CONTRACT_ADDRESS_UNAVAILABLE")

    derivatives = fetch_derivatives(sample)
    raw["derivatives"] = derivatives
    merge_derivatives(event, derivatives)
    if start_ts is not None and end_ts is not None:
        history = fetch_coinalyze_history(
            sample, coinalyze_key, start_ts, end_ts, coinalyze_interval
        )
        raw["coinalyze_history"] = history
        merge_coinalyze_history(event, history)
    if start_ts is not None and end_ts is not None:
        cex_flow = fetch_cex_flow(
            sample,
            etherscan_key,
            cex_labels_path,
            start_ts,
            end_ts,
            reference_price_usd,
            bsc_rpc_url,
        )
        raw["cex_flow"] = cex_flow
        merge_cex_flow(event, cex_flow)
    return event, raw


def fetch_goplus_security(contract_address: str) -> dict[str, Any]:
    url = f"{GOPLUS_URL}?{urllib.parse.urlencode({'contract_addresses': contract_address})}"
    return http_json(url)


def fetch_derivatives(sample: dict[str, Any]) -> dict[str, Any]:
    exchanges = []
    for symbol in sample.get("perp_symbols", [_default_perp_symbol(sample["symbol"])]):
        exchanges.append(fetch_bitget_derivatives(symbol))
        exchanges.append(fetch_gate_derivatives(_gate_contract(symbol)))
    return {"exchanges": exchanges}


def fetch_bitget_derivatives(symbol: str) -> dict[str, Any]:
    result = {"exchange": "bitget", "symbol": symbol, "available": False}
    oi_url = f"{BITGET_BASE}/open-interest?{_query({'symbol': symbol, 'productType': 'USDT-FUTURES'})}"
    funding_url = (
        f"{BITGET_BASE}/history-fund-rate?"
        f"{_query({'symbol': symbol, 'productType': 'USDT-FUTURES', 'pageSize': '5'})}"
    )
    oi = http_json(oi_url)
    funding = http_json(funding_url)
    result["raw_open_interest"] = oi
    result["raw_funding"] = funding
    if oi.get("code") != "00000" and funding.get("code") != "00000":
        result["error"] = oi.get("msg") or funding.get("msg")
        return result

    result["available"] = True
    oi_list = (oi.get("data") or {}).get("openInterestList") or []
    if oi_list:
        result["open_interest"] = oi_list[0].get("size")
    rates = funding.get("data") or []
    if rates:
        result["funding_rate"] = rates[0].get("fundingRate")
    return result


def fetch_gate_derivatives(contract: str) -> dict[str, Any]:
    result = {"exchange": "gate", "symbol": contract, "available": False}
    contract_url = f"{GATE_BASE}/contracts/{contract}"
    funding_url = f"{GATE_BASE}/funding_rate?{_query({'contract': contract, 'limit': 5})}"
    info = http_json(contract_url)
    funding = http_json(funding_url)
    result["raw_contract"] = info
    result["raw_funding"] = funding
    if info.get("label") == "CONTRACT_NOT_FOUND":
        result["error"] = "CONTRACT_NOT_FOUND"
        return result

    result["available"] = True
    result["open_interest"] = info.get("position_size")
    result["funding_rate"] = info.get("funding_rate")
    result["short_crowding_score"] = _short_crowding(info)
    return result


def http_json(url: str, timeout: int = 20) -> dict[str, Any]:
    req = urllib.request.Request(url, headers={"User-Agent": "rust-quant-backtest/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception:
        raw = subprocess.check_output(
            ["curl", "-L", "--max-time", str(timeout), "-s", url],
            timeout=timeout + 5,
        )
        if not raw:
            return {"error": "EMPTY_RESPONSE"}
        return json.loads(raw.decode("utf-8"))


def _default_perp_symbol(spot_symbol: str) -> str:
    base = spot_symbol.split("_", 1)[0].upper()
    return f"{base}USDT"


def _gate_contract(symbol: str) -> str:
    if symbol.endswith("USDT"):
        return f"{symbol[:-4]}_USDT"
    return symbol


def _query(params: dict[str, str | int]) -> str:
    return urllib.parse.urlencode(params)


def _short_crowding(info: dict[str, Any]) -> float | None:
    try:
        long_users = float(info.get("long_users") or 0.0)
        short_users = float(info.get("short_users") or 0.0)
    except (TypeError, ValueError):
        return None
    total = long_users + short_users
    return short_users / total if total > 0 else None
