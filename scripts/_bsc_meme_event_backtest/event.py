from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .core import BacktestConfig, BacktestResult, Candle, entry_signal, simulate_trade


@dataclass
class EventSnapshot:
    symbol: str
    contract_address: str | None = None
    event_tags: list[str] = field(default_factory=list)
    security_checked: bool = False
    sell_simulation_passed: bool = False
    buy_tax_pct: float = 0.0
    sell_tax_pct: float = 0.0
    has_blacklist_risk: bool = False
    has_pause_risk: bool = False
    has_mint_risk: bool = False
    dex_liquidity_usd: float = 0.0
    top1_holder_pct: float = 0.0
    top10_holder_pct: float = 0.0
    derivatives_checked: bool = False
    perp_exchanges: list[str] = field(default_factory=list)
    current_open_interest: float | None = None
    funding_rate: float | None = None
    short_crowding_score: float = 0.0
    historical_oi_available: bool = False
    oi_growth_1h_pct: float | None = None
    oi_growth_4h_pct: float | None = None
    cex_flow_checked: bool = False
    cex_net_inflow_usd: float | None = None
    cex_outflow_after_inflow: bool = False
    spot_absorption: bool = False
    warnings: list[str] = field(default_factory=list)


def parse_goplus_security(
    symbol: str, contract_address: str, payload: dict[str, Any]
) -> EventSnapshot:
    token = _token_payload(contract_address, payload)
    snapshot = EventSnapshot(symbol=symbol, contract_address=contract_address)
    if not token:
        snapshot.warnings.append("GOPLUS_SECURITY_MISSING")
        return snapshot

    snapshot.security_checked = True
    snapshot.buy_tax_pct = _decimal_pct(token.get("buy_tax"))
    snapshot.sell_tax_pct = _decimal_pct(token.get("sell_tax"))
    snapshot.sell_simulation_passed = _is_zero(token.get("is_honeypot"))
    snapshot.sell_simulation_passed &= _is_zero(token.get("cannot_sell_all"))
    snapshot.sell_simulation_passed &= _is_zero(token.get("cannot_buy"))
    snapshot.has_blacklist_risk = _is_one(token.get("is_blacklisted"))
    snapshot.has_pause_risk = _is_one(token.get("transfer_pausable"))
    snapshot.has_mint_risk = _is_one(token.get("is_mintable"))
    snapshot.dex_liquidity_usd = sum(
        (_float(d.get("liquidity")) or 0.0) for d in token.get("dex", [])
    )
    holders = token.get("holders", [])
    percents = [_fraction_pct(h.get("percent")) for h in holders]
    snapshot.top1_holder_pct = percents[0] if percents else 0.0
    snapshot.top10_holder_pct = sum(percents[:10])
    return snapshot


def merge_derivatives(snapshot: EventSnapshot, data: dict[str, Any]) -> EventSnapshot:
    valid = [item for item in data.get("exchanges", []) if item.get("available")]
    if not valid:
        snapshot.warnings.append("DERIVATIVES_MARKET_MISSING")
        return snapshot

    snapshot.derivatives_checked = True
    snapshot.perp_exchanges = [item["exchange"] for item in valid]
    funding_rates = [_float(item.get("funding_rate")) for item in valid]
    funding_rates = [rate for rate in funding_rates if rate is not None]
    if funding_rates:
        snapshot.funding_rate = min(funding_rates)
    oi_values = [_float(item.get("open_interest")) for item in valid]
    oi_values = [value for value in oi_values if value is not None]
    if oi_values:
        snapshot.current_open_interest = max(oi_values)
    crowding = [_float(item.get("short_crowding_score")) for item in valid]
    crowding = [value for value in crowding if value is not None]
    if crowding:
        snapshot.short_crowding_score = max(crowding)
    if not snapshot.historical_oi_available:
        snapshot.warnings.append("HISTORICAL_OI_GROWTH_UNAVAILABLE")
    return snapshot


def merge_coinalyze_history(snapshot: EventSnapshot, data: dict[str, Any]) -> EventSnapshot:
    if not data.get("available"):
        error = data.get("error", "COINALYZE_HISTORY_UNAVAILABLE")
        snapshot.warnings.append(error)
        return snapshot

    snapshot.derivatives_checked = True
    if "coinalyze" not in snapshot.perp_exchanges:
        snapshot.perp_exchanges.append("coinalyze")
    snapshot.historical_oi_available = True
    snapshot.oi_growth_1h_pct = data.get("oi_growth_1h_pct")
    snapshot.oi_growth_4h_pct = data.get("oi_growth_4h_pct")
    if data.get("funding_rate") is not None:
        snapshot.funding_rate = data["funding_rate"]
    if data.get("short_crowding_score") is not None:
        snapshot.short_crowding_score = max(
            snapshot.short_crowding_score, data["short_crowding_score"]
        )
    return snapshot


def merge_cex_flow(snapshot: EventSnapshot, data: dict[str, Any]) -> EventSnapshot:
    if not data.get("available"):
        snapshot.warnings.append(data.get("error", "CEX_FLOW_UNAVAILABLE"))
        return snapshot
    snapshot.cex_flow_checked = True
    snapshot.cex_net_inflow_usd = data.get("net_inflow_usd")
    snapshot.cex_outflow_after_inflow = bool(data.get("outflow_after_inflow"))
    snapshot.spot_absorption = data.get("net_inflow_usd", 0.0) <= 0.0
    return snapshot


def event_blockers(event: EventSnapshot, cfg: BacktestConfig, strict: bool) -> list[str]:
    blockers: list[str] = []
    if not event.contract_address:
        blockers.append("CONTRACT_ADDRESS_MISSING")
    if not event.event_tags:
        blockers.append("EVENT_TAG_MISSING")
    if not event.security_checked:
        blockers.append("SECURITY_DATA_MISSING")
    if event.security_checked and not event.sell_simulation_passed:
        blockers.append("CONTRACT_SECURITY_BLOCK")
    if event.buy_tax_pct > cfg.max_tax_pct:
        blockers.append("BUY_TAX_TOO_HIGH")
    if event.sell_tax_pct > cfg.max_tax_pct:
        blockers.append("SELL_TAX_TOO_HIGH")
    if event.has_blacklist_risk or event.has_pause_risk or event.has_mint_risk:
        blockers.append("CONTRACT_PRIVILEGE_RISK")
    if event.security_checked and event.dex_liquidity_usd < cfg.min_depth_2pct_usd:
        blockers.append("DEX_LIQUIDITY_TOO_THIN")
    if strict and not event.derivatives_checked:
        blockers.append("DERIVATIVES_DATA_MISSING")
    if strict and not event.historical_oi_available:
        blockers.append("HISTORICAL_OI_GROWTH_MISSING")
    if strict and not event.cex_flow_checked:
        blockers.append("CEX_FLOW_DATA_MISSING")
    return blockers


def run_full_event_replay(
    symbol: str,
    candles: list[Candle],
    event: EventSnapshot,
    cfg: BacktestConfig,
    strict: bool = True,
) -> BacktestResult:
    result = BacktestResult(symbol=symbol, bars=len(candles))
    blockers = event_blockers(event, cfg, strict)
    if blockers:
        result.data_warning = ",".join(blockers + event.warnings)
        return result

    for i in range(len(candles)):
        if full_event_entry_signal(candles, i, event, cfg):
            trade = simulate_trade(symbol, candles, i, cfg)
            if event.warnings:
                trade.data_warning = ",".join(event.warnings)
            return trade
    if event.warnings:
        result.data_warning = ",".join(event.warnings)
    return result


def full_event_entry_signal(
    candles: list[Candle], i: int, event: EventSnapshot, cfg: BacktestConfig
) -> bool:
    return (
        entry_signal(candles, i, cfg)
        and _has_squeeze(event, cfg)
        and _whale_flow_ok(event, cfg)
    )


def _has_squeeze(event: EventSnapshot, cfg: BacktestConfig) -> bool:
    oi_1h = event.oi_growth_1h_pct or 0.0
    oi_4h = event.oi_growth_4h_pct or 0.0
    oi_growth = oi_1h >= cfg.min_oi_growth_1h_pct or oi_4h >= cfg.min_oi_growth_4h_pct
    funding = event.funding_rate if event.funding_rate is not None else 0.0
    crowded = funding < 0.0 or event.short_crowding_score >= cfg.min_short_crowding_score
    return event.historical_oi_available and oi_growth and crowded


def _whale_flow_ok(event: EventSnapshot, cfg: BacktestConfig) -> bool:
    if not event.cex_flow_checked:
        return False
    inflow = event.cex_net_inflow_usd or 0.0
    return inflow < cfg.large_cex_flow_usd or event.cex_outflow_after_inflow or event.spot_absorption


def _token_payload(contract_address: str, payload: dict[str, Any]) -> dict[str, Any] | None:
    result = payload.get("result", {})
    return result.get(contract_address) or result.get(contract_address.lower())


def _is_one(value: Any) -> bool:
    return str(value).strip() == "1"


def _is_zero(value: Any) -> bool:
    return str(value).strip() == "0"


def _float(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _decimal_pct(value: Any) -> float:
    number = _float(value)
    return 0.0 if number is None else number * 100.0


def _fraction_pct(value: Any) -> float:
    number = _float(value)
    return 0.0 if number is None else number * 100.0
