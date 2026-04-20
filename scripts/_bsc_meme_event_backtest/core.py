from __future__ import annotations

import math
from dataclasses import dataclass


@dataclass
class Candle:
    ts: int
    open: float
    high: float
    low: float
    close: float
    volume: float

    @property
    def quote_volume(self) -> float:
        return self.close * self.volume


@dataclass
class BacktestConfig:
    min_volume_24h_usd: float = 5_000_000.0
    min_volume_1h_vs_24h_avg: float = 5.0
    min_price_change_15m_pct: float = 8.0
    min_price_change_1h_pct: float = 20.0
    min_volume_zscore: float = 3.0
    min_depth_2pct_usd: float = 50_000.0
    min_oi_growth_1h_pct: float = 30.0
    min_oi_growth_4h_pct: float = 80.0
    min_short_crowding_score: float = 0.65
    max_tax_pct: float = 5.0
    large_cex_flow_usd: float = 250_000.0
    stop_loss_pct: float = -10.0
    first_take_profit_pct: float = 25.0
    second_take_profit_pct: float = 60.0
    trailing_stop_pct: float = 20.0
    time_stop_minutes: int = 30
    min_time_stop_profit_pct: float = 8.0
    max_hold_minutes: int = 24 * 60
    cost_r: float = 0.15
    min_history_bars: int = 24


@dataclass
class BacktestResult:
    symbol: str
    entered: bool = False
    entry_ts: int | None = None
    exit_ts: int | None = None
    entry_price: float | None = None
    exit_price: float | None = None
    exit_reason: str = "NO_ENTRY"
    gross_r: float = 0.0
    net_r: float = 0.0
    max_profit_pct: float = 0.0
    max_loss_pct: float = 0.0
    bars: int = 0
    data_warning: str | None = None


def pct_change(now: float, prev: float) -> float:
    return (now / prev - 1.0) * 100.0 if prev > 0 else 0.0


def vwap(candles: list[Candle]) -> float:
    volume = sum(c.volume for c in candles)
    return sum(c.close * c.volume for c in candles) / volume if volume > 0 else 0.0


def zscore(value: float, history: list[float]) -> float:
    if len(history) < 3:
        return 0.0
    mean = sum(history) / len(history)
    variance = sum((x - mean) ** 2 for x in history) / len(history)
    std = math.sqrt(variance)
    return (value - mean) / std if std > 0 else 0.0


def entry_signal(candles: list[Candle], i: int, cfg: BacktestConfig) -> bool:
    if i < max(cfg.min_history_bars, 12):
        return False
    current = candles[i]
    history = candles[max(0, i - 287) : i + 1]
    quote_24h = sum(c.quote_volume for c in history)
    quote_1h = sum(c.quote_volume for c in candles[max(0, i - 11) : i + 1])
    hours = max(len(history) / 12.0, 1.0)
    avg_hour = quote_24h / hours if quote_24h > 0 else 0.0
    volume_ratio = quote_1h / avg_hour if avg_hour > 0 else 0.0
    vol_condition = quote_24h >= cfg.min_volume_24h_usd
    vol_condition = vol_condition or volume_ratio >= cfg.min_volume_1h_vs_24h_avg
    price_15m = pct_change(current.close, candles[i - 3].close)
    price_1h = pct_change(current.close, candles[i - 12].close)
    recent_vwap = vwap(candles[i - 2 : i + 1])
    vol_z = zscore(current.quote_volume, [c.quote_volume for c in history[:-1]])
    return (
        vol_condition
        and price_15m >= cfg.min_price_change_15m_pct
        and price_1h >= cfg.min_price_change_1h_pct
        and current.close > recent_vwap
        and vol_z >= cfg.min_volume_zscore
    )


def run_price_volume_replay(
    symbol: str, candles: list[Candle], cfg: BacktestConfig
) -> BacktestResult:
    result = BacktestResult(symbol=symbol, bars=len(candles))
    for i in range(len(candles)):
        if entry_signal(candles, i, cfg):
            return simulate_trade(symbol, candles, i, cfg)
    return result


def simulate_trade(
    symbol: str, candles: list[Candle], entry_i: int, cfg: BacktestConfig
) -> BacktestResult:
    entry = candles[entry_i]
    stop = entry.close * (1.0 + cfg.stop_loss_pct / 100.0)
    tp1 = entry.close * (1.0 + cfg.first_take_profit_pct / 100.0)
    tp2 = entry.close * (1.0 + cfg.second_take_profit_pct / 100.0)
    r_unit = entry.close - stop
    realized_r = 0.0
    remaining = 1.0
    hit_tp1 = False
    hit_tp2 = False
    max_high = entry.close
    result = BacktestResult(
        symbol=symbol, entered=True, entry_ts=entry.ts, entry_price=entry.close, bars=len(candles)
    )
    max_hold_bars = max(1, cfg.max_hold_minutes // 5)
    time_stop_bars = max(1, cfg.time_stop_minutes // 5)
    for j, candle in enumerate(candles[entry_i + 1 : entry_i + max_hold_bars + 1], start=1):
        max_high = max(max_high, candle.high)
        result.max_profit_pct = max(result.max_profit_pct, pct_change(max_high, entry.close))
        result.max_loss_pct = min(result.max_loss_pct, pct_change(candle.low, entry.close))
        if candle.low <= stop:
            return finalize(result, candle, "STOP_LOSS", realized_r - remaining, cfg)
        if not hit_tp1 and candle.high >= tp1:
            realized_r += (1.0 / 3.0) * ((tp1 - entry.close) / r_unit)
            remaining -= 1.0 / 3.0
            hit_tp1 = True
        if not hit_tp2 and candle.high >= tp2:
            realized_r += (1.0 / 3.0) * ((tp2 - entry.close) / r_unit)
            remaining -= 1.0 / 3.0
            hit_tp2 = True
        trailing = max_high * (1.0 - cfg.trailing_stop_pct / 100.0)
        if hit_tp1 and candle.low <= trailing:
            extra_r = remaining * ((trailing - entry.close) / r_unit)
            return finalize(result, candle, "TRAILING_STOP", realized_r + extra_r, cfg, trailing)
        if j >= time_stop_bars and result.max_profit_pct < cfg.min_time_stop_profit_pct:
            extra_r = remaining * ((candle.close - entry.close) / r_unit)
            return finalize(result, candle, "TIME_STOP", realized_r + extra_r, cfg)
    last = candles[min(len(candles) - 1, entry_i + max_hold_bars)]
    extra_r = remaining * ((last.close - entry.close) / r_unit)
    return finalize(result, last, "MAX_HOLD", realized_r + extra_r, cfg)


def finalize(result, candle, reason, gross_r, cfg, exit_price=None):
    result.exit_ts = candle.ts
    result.exit_price = candle.close if exit_price is None else exit_price
    result.exit_reason = reason
    result.gross_r = gross_r
    result.net_r = gross_r - cfg.cost_r
    return result


def summarize_results(results: list[BacktestResult]) -> dict:
    trades = [r for r in results if r.entered]
    winners = [r for r in trades if r.net_r > 0]
    losers = [r for r in trades if r.net_r <= 0]
    net_r = sum(r.net_r for r in trades)
    gross_win = sum(r.net_r for r in winners)
    gross_loss = abs(sum(r.net_r for r in losers))
    largest = max((r.net_r for r in trades), default=0.0)
    without_largest = net_r - largest if trades else 0.0
    avg_win = gross_win / len(winners) if winners else 0.0
    avg_loss = gross_loss / len(losers) if losers else 0.0
    win_rate = len(winners) / len(trades) if trades else 0.0
    pf = gross_win / gross_loss if gross_loss > 0 else (float("inf") if gross_win > 0 else 0.0)
    return {
        "samples": len(results),
        "trades": len(trades),
        "wins": len(winners),
        "losses": len(losers),
        "win_rate": win_rate,
        "avg_win_r": avg_win,
        "avg_loss_r": avg_loss,
        "profit_factor": pf,
        "net_r": net_r,
        "avg_net_r": net_r / len(trades) if trades else 0.0,
        "largest_winner_r": largest,
        "net_r_without_largest_winner": without_largest,
        "passes_proof_gate": proof_gate(len(trades), win_rate, avg_win, avg_loss, net_r, pf, without_largest),
    }


def proof_gate(trades, win_rate, avg_win, avg_loss, net_r, pf, without_largest):
    return (
        trades >= 10
        and win_rate >= 0.42
        and avg_win >= 2.0
        and avg_loss <= 1.0
        and (net_r / trades) >= 0.25
        and pf >= 1.35
        and without_largest > 0
    )
