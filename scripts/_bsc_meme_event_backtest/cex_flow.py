from __future__ import annotations

import json
import os
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from .bsc_rpc import (
    DEFAULT_BSC_RPC_URL,
    fetch_token_transfers_rpc,
    is_rpc_archive_required_error,
    rpc_block_by_timestamp,
)
from .http_json import curl_json, curl_proxy

ETHERSCAN_V2_URL = "https://api.etherscan.io/v2/api"


def fetch_cex_flow(
    sample: dict[str, Any],
    api_key: str | None,
    labels_path: str | None,
    start_ts: int,
    end_ts: int,
    reference_price_usd: float,
    rpc_url: str | None = None,
) -> dict[str, Any]:
    contract = sample.get("contract_address")
    if not contract:
        return {"available": False, "error": "CONTRACT_ADDRESS_MISSING"}
    labels = load_cex_labels(labels_path)
    if labels is None:
        return {"available": False, "error": "CEX_WALLET_LABELS_MISSING"}
    if not labels:
        return {"available": False, "error": "CEX_WALLET_LABELS_EMPTY"}

    flow = fetch_transfers_rpc_first(contract, start_ts, end_ts, api_key, rpc_url)
    if not flow.get("available"):
        return flow
    transfers = flow["transfers"]
    start_block = flow["start_block"]
    end_block = flow["end_block"]
    summary = summarize_cex_flow(transfers, labels, reference_price_usd)
    summary.update(
        {
            "available": True,
            "contract_address": contract,
            "source": flow["source"],
            "start_block": start_block,
            "end_block": end_block,
            "reference_price_usd": reference_price_usd,
            "raw_transfer_count": len(transfers),
        }
    )
    return summary


def fetch_cex_counterparty_candidates(
    sample: dict[str, Any],
    api_key: str | None,
    start_ts: int,
    end_ts: int,
    reference_price_usd: float,
    top_n: int = 50,
    rpc_url: str | None = None,
) -> dict[str, Any]:
    contract = sample.get("contract_address")
    if not contract:
        return {"available": False, "error": "CONTRACT_ADDRESS_MISSING"}
    flow = fetch_transfers_rpc_first(contract, start_ts, end_ts, api_key, rpc_url)
    if not flow.get("available"):
        return flow
    transfers = flow["transfers"]
    return {
        "available": True,
        "contract_address": contract,
        "source": flow["source"],
        "start_block": flow["start_block"],
        "end_block": flow["end_block"],
        "raw_transfer_count": len(transfers),
        "candidates": summarize_counterparty_candidates(
            transfers, reference_price_usd, top_n=top_n
        ),
    }


def load_cex_labels(labels_path: str | None) -> dict[str, str] | None:
    if not labels_path:
        return None
    path = Path(labels_path)
    if not path.exists():
        return None
    payload = json.loads(path.read_text())
    raw = payload.get("addresses", payload)
    labels: dict[str, str] = {}
    if isinstance(raw, dict):
        for address, value in raw.items():
            label = value.get("label") if isinstance(value, dict) else value
            labels[address.lower()] = str(label)
    elif isinstance(raw, list):
        for item in raw:
            address = str(item.get("address", "")).lower()
            if address:
                labels[address] = str(item.get("label", "cex_wallet"))
    return labels


def summarize_counterparty_candidates(
    transfers: list[dict[str, Any]], reference_price_usd: float, top_n: int = 50
) -> list[dict[str, Any]]:
    stats: dict[str, dict[str, Any]] = {}
    for transfer in transfers:
        amount = transfer_amount(transfer)
        for side, key in (("sent", "from"), ("received", "to")):
            address = str(transfer.get(key, "")).lower()
            if not address:
                continue
            row = stats.setdefault(
                address,
                {
                    "address": address,
                    "sent_tokens": 0.0,
                    "received_tokens": 0.0,
                    "tx_count": 0,
                },
            )
            row[f"{side}_tokens"] += amount
            row["tx_count"] += 1

    rows = []
    for row in stats.values():
        sent = row["sent_tokens"]
        received = row["received_tokens"]
        volume = sent + received
        row["volume_tokens"] = volume
        row["net_received_tokens"] = received - sent
        row["volume_usd"] = volume * reference_price_usd
        row["net_received_usd"] = row["net_received_tokens"] * reference_price_usd
        rows.append(row)
    rows.sort(key=lambda item: (item["volume_usd"], item["tx_count"]), reverse=True)
    return rows[:top_n]


def summarize_cex_flow(
    transfers: list[dict[str, Any]], labels: dict[str, str], reference_price_usd: float
) -> dict[str, Any]:
    inflow_tokens = 0.0
    outflow_tokens = 0.0
    inflow_count = 0
    outflow_count = 0
    touched_labels: set[str] = set()
    for transfer in transfers:
        from_addr = str(transfer.get("from", "")).lower()
        to_addr = str(transfer.get("to", "")).lower()
        amount = transfer_amount(transfer)
        if to_addr in labels:
            inflow_tokens += amount
            inflow_count += 1
            touched_labels.add(labels[to_addr])
        if from_addr in labels:
            outflow_tokens += amount
            outflow_count += 1
            touched_labels.add(labels[from_addr])

    inflow_usd = inflow_tokens * reference_price_usd
    outflow_usd = outflow_tokens * reference_price_usd
    return {
        "inflow_tokens": inflow_tokens,
        "outflow_tokens": outflow_tokens,
        "net_inflow_tokens": inflow_tokens - outflow_tokens,
        "inflow_usd": inflow_usd,
        "outflow_usd": outflow_usd,
        "net_inflow_usd": inflow_usd - outflow_usd,
        "inflow_count": inflow_count,
        "outflow_count": outflow_count,
        "outflow_after_inflow": outflow_count > 0 and outflow_usd >= inflow_usd,
        "labels": sorted(touched_labels),
    }


def transfer_amount(transfer: dict[str, Any]) -> float:
    value = float(transfer.get("value") or 0.0)
    decimals = int(transfer.get("tokenDecimal") or 0)
    return value / (10**decimals)


def fetch_transfers_rpc_first(
    contract_address: str,
    start_ts: int,
    end_ts: int,
    api_key: str | None,
    rpc_url: str | None = None,
) -> dict[str, Any]:
    rpc = rpc_url or os.environ.get("BSC_RPC_URL") or DEFAULT_BSC_RPC_URL
    try:
        start_block = rpc_block_by_timestamp(rpc, start_ts)
        end_block = rpc_block_by_timestamp(rpc, end_ts)
        transfers = fetch_token_transfers_rpc(rpc, contract_address, start_block, end_block)
        return {
            "available": True,
            "source": "bsc_rpc",
            "start_block": start_block,
            "end_block": end_block,
            "transfers": transfers,
        }
    except Exception as rpc_exc:
        rpc_error_code = (
            "BSC_RPC_ARCHIVE_REQUIRED"
            if is_rpc_archive_required_error(rpc_exc)
            else "BSC_RPC_REQUEST_FAILED"
        )
        if not api_key:
            return {
                "available": False,
                "error": rpc_error_code,
                "error_detail": type(rpc_exc).__name__,
                "rpc_error": str(rpc_exc)[:200],
            }
        try:
            start_block = get_block_by_timestamp(api_key, start_ts, "before")
            end_block = get_block_by_timestamp(api_key, end_ts, "after")
            transfers = fetch_token_transfers(api_key, contract_address, start_block, end_block)
            return {
                "available": True,
                "source": "etherscan_v2",
                "start_block": start_block,
                "end_block": end_block,
                "transfers": transfers,
            }
        except Exception as eth_exc:
            return {
                "available": False,
                "error": "TRANSFER_FETCH_FAILED",
                "rpc_error": rpc_error_code,
                "rpc_error_detail": str(rpc_exc)[:200],
                "etherscan_error": type(eth_exc).__name__,
                "etherscan_error_detail": str(eth_exc)[:200],
            }


def get_block_by_timestamp(api_key: str, timestamp: int, closest: str) -> int:
    payload = etherscan_get(
        api_key,
        {
            "module": "block",
            "action": "getblocknobytime",
            "timestamp": timestamp,
            "closest": closest,
        },
    )
    if payload.get("status") != "1":
        raise RuntimeError(payload.get("result") or "ETHERSCAN_BLOCK_LOOKUP_FAILED")
    return int(payload["result"])


def fetch_token_transfers(
    api_key: str, contract_address: str, start_block: int, end_block: int
) -> list[dict[str, Any]]:
    transfers: list[dict[str, Any]] = []
    for page in range(1, 6):
        payload = etherscan_get(
            api_key,
            {
                "module": "account",
                "action": "tokentx",
                "contractaddress": contract_address,
                "startblock": start_block,
                "endblock": end_block,
                "page": page,
                "offset": 10000,
                "sort": "asc",
            },
        )
        result = payload.get("result")
        if payload.get("status") != "1" or not isinstance(result, list):
            break
        transfers.extend(result)
        if len(result) < 10000:
            break
    return transfers


def etherscan_get(api_key: str, params: dict[str, Any]) -> dict[str, Any]:
    query = urllib.parse.urlencode({**params, "chainid": "56", "apikey": api_key})
    url = f"{ETHERSCAN_V2_URL}?{query}"
    proxy = curl_proxy()
    if proxy:
        return curl_json(url, proxy)
    req = urllib.request.Request(url, headers={"User-Agent": "rust-quant-backtest/1.0"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))
