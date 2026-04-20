from __future__ import annotations

import json
import time
import urllib.parse
import urllib.request

from .core import Candle

LBANK_KLINE_URL = "https://api.lbkex.com/v2/kline.do"
INTERVAL_SECONDS = 300


def default_samples() -> list[dict]:
    return [
        {
            "symbol": "rave_usdt",
            "group": "success",
            "contract_address": "0x97693439ea2f0ecdeb9135881e49f354656a911c",
            "event_tags": ["cex_listing", "top_gainer"],
            "perp_symbols": ["RAVEUSDT"],
        },
        {
            "symbol": "bianrensheng_usdt",
            "group": "success",
            "contract_address": "0x924fa68a0fc644485b8df8abfa0a41c2e7744444",
            "event_tags": ["binance_alpha", "four_meme", "top_gainer"],
        },
        {
            "symbol": "4_usdt",
            "group": "success",
            "contract_address": "0x0a43fc31a73013089df59194872ecae4cae14444",
            "event_tags": ["four_meme", "cex_listing", "top_gainer"],
            "perp_symbols": ["4USDT"],
        },
        {
            "symbol": "palu_usdt",
            "group": "success",
            "contract_address": "0x02e75d28a8aa2a0033b8cf866fcf0bb0e1ee4444",
            "event_tags": ["four_meme", "cex_listing", "top_gainer"],
            "perp_symbols": ["PALUUSDT"],
        },
        {
            "symbol": "kefuxiaohe_usdt",
            "group": "success",
            "contract_address": "0x3ac8e2c113d5d7824ac6ebe82a3c60b1b9d64444",
            "event_tags": ["four_meme", "cex_listing"],
        },
        {
            "symbol": "xiuxian_usdt",
            "group": "success",
            "contract_address": "0x44443dd87ec4d1bea3425acc118adb023f07f91b",
            "event_tags": ["four_meme", "cex_listing"],
        },
        {
            "symbol": "hajimi_usdt",
            "group": "success",
            "contract_address": "0x82ec31d69b3c289e541b50e30681fd1acad24444",
            "event_tags": ["four_meme", "cex_listing"],
        },
        {"symbol": "broccoli4_usdt", "group": "control", "event_tags": []},
        {
            "symbol": "bnbholder_usdt",
            "group": "control",
            "event_tags": ["cex_listing"],
            "perp_symbols": ["BNBHOLDERUSDT"],
        },
        {"symbol": "ai4_usdt", "group": "control", "event_tags": []},
        {"symbol": "npcz_usdt", "group": "control", "event_tags": []},
        {
            "symbol": "giggle_usdt",
            "group": "control",
            "contract_address": "0x20d6015660b3fe52e6690a889b5c51f69902ce0e",
            "event_tags": ["cex_listing", "four_meme"],
            "perp_symbols": ["GIGGLEUSDT"],
        },
    ]


def http_json(url: str, timeout: int = 20) -> dict:
    req = urllib.request.Request(url, headers={"User-Agent": "rust-quant-backtest/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def lbank_url(symbol: str, start_ts: int, size: int) -> str:
    query = urllib.parse.urlencode(
        {"symbol": symbol, "type": "minute5", "size": size, "time": start_ts}
    )
    return f"{LBANK_KLINE_URL}?{query}"


def parse_lbank_rows(rows: list) -> list[Candle]:
    return [
        Candle(
            ts=int(row[0]),
            open=float(row[1]),
            high=float(row[2]),
            low=float(row[3]),
            close=float(row[4]),
            volume=float(row[5]),
        )
        for row in rows
    ]


def fetch_lbank_klines(symbol: str, start_ts: int, end_ts: int) -> list[Candle]:
    candles: list[Candle] = []
    cursor = start_ts
    seen = set()
    while cursor <= end_ts:
        payload = http_json(lbank_url(symbol, cursor, 2000))
        rows = payload.get("data") or []
        batch = [c for c in parse_lbank_rows(rows) if start_ts <= c.ts <= end_ts]
        for candle in [c for c in batch if c.ts not in seen]:
            seen.add(candle.ts)
            candles.append(candle)
        if not rows:
            cursor += 24 * 3600
            continue
        last_ts = int(rows[-1][0])
        next_cursor = last_ts + INTERVAL_SECONDS
        if next_cursor <= cursor:
            break
        cursor = next_cursor
        if len(rows) < 2000 and cursor > int(time.time()):
            break
    return sorted(candles, key=lambda c: c.ts)


def discover_first_candle(symbol: str, start_ts: int, end_ts: int) -> int | None:
    payload = http_json(lbank_url(symbol, start_ts, 1))
    rows = payload.get("data") or []
    if not rows:
        return None
    first_ts = int(rows[0][0])
    return first_ts if first_ts <= end_ts else None
