from __future__ import annotations

import json
import os
import time
from typing import Any

from .http_json import curl_json_rpc, curl_proxy, urllib_json_rpc

DEFAULT_BSC_RPC_URL = "https://bsc-dataseed.binance.org"
TRANSFER_TOPIC = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"


def rpc_block_by_timestamp(rpc_url: str, target_ts: int) -> int:
    latest = rpc_block_number(rpc_url)
    low = max(1, latest - int((int(time_now()) - target_ts) / 3) - 100_000)
    high = latest
    best = low
    while low <= high:
        mid = (low + high) // 2
        block = rpc_get_block(rpc_url, mid)
        ts = int(block["timestamp"], 16)
        if ts <= target_ts:
            best = mid
            low = mid + 1
        else:
            high = mid - 1
    return best


def rpc_block_number(rpc_url: str) -> int:
    return int(rpc_call(rpc_url, "eth_blockNumber", []), 16)


def rpc_get_block(rpc_url: str, block_number: int) -> dict[str, Any]:
    block = rpc_call(rpc_url, "eth_getBlockByNumber", [hex(block_number), False])
    if not block:
        raise RuntimeError("RPC_BLOCK_NOT_FOUND")
    return block


def fetch_token_transfers_rpc(
    rpc_url: str, contract_address: str, start_block: int, end_block: int
) -> list[dict[str, Any]]:
    transfers: list[dict[str, Any]] = []
    step = int(os.environ.get("BSC_RPC_LOG_BLOCK_STEP", "2000"))
    current = start_block
    while current <= end_block:
        to_block = min(current + step - 1, end_block)
        try:
            logs = rpc_get_transfer_logs(rpc_url, contract_address, current, to_block)
        except RuntimeError as exc:
            if step <= 1 or not is_rpc_limit_error(exc):
                raise
            step = max(1, step // 2)
            continue
        transfers.extend(decode_transfer_log(log) for log in logs)
        current = to_block + 1
    return transfers


def rpc_get_transfer_logs(
    rpc_url: str, contract_address: str, from_block: int, to_block: int
) -> list[dict[str, Any]]:
    return rpc_call(
        rpc_url,
        "eth_getLogs",
        [
            {
                "address": contract_address,
                "fromBlock": hex(from_block),
                "toBlock": hex(to_block),
                "topics": [TRANSFER_TOPIC],
            }
        ],
    )


def rpc_call(rpc_url: str, method: str, params: list[Any]) -> Any:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    proxy = curl_proxy()
    payload = curl_json_rpc(rpc_url, body, proxy) if proxy else urllib_json_rpc(rpc_url, body)
    if payload.get("error"):
        raise RuntimeError(payload["error"])
    return payload.get("result")


def decode_transfer_log(log: dict[str, Any]) -> dict[str, Any]:
    topics = log.get("topics") or []
    if len(topics) < 3:
        raise RuntimeError("TRANSFER_LOG_TOPICS_MISSING")
    return {
        "from": topic_to_address(topics[1]),
        "to": topic_to_address(topics[2]),
        "value": str(int(log.get("data", "0x0"), 16)),
        "tokenDecimal": "18",
        "blockNumber": int(log.get("blockNumber", "0x0"), 16),
        "transactionHash": log.get("transactionHash"),
        "logIndex": int(log.get("logIndex", "0x0"), 16),
    }


def topic_to_address(topic: str) -> str:
    clean = topic.removeprefix("0x")
    return "0x" + clean[-40:].lower()


def is_rpc_limit_error(exc: RuntimeError) -> bool:
    text = str(exc).lower()
    return "limit exceeded" in text or "more than" in text or "too many" in text


def is_rpc_archive_required_error(exc: Exception) -> bool:
    text = str(exc).lower()
    return (
        ("archive" in text and "not available" in text)
        or "missing trie node" in text
        or "pruned" in text
    )


def time_now() -> float:
    return time.time()
