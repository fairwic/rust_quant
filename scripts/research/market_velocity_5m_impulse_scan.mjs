#!/usr/bin/env node

import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

const DEFAULT_RANK_CACHE = '/tmp/mv5m_rankhot30_20260601_20260703_1615.json';
const DEFAULT_MACRO_CACHE = '/tmp/mv5m_macro_btc_eth_20260601_20260703_1615.json';
const FIVE_MINUTE_MS = 5 * 60 * 1000;
const HOLDING_CANDLES = (48 * 60) / 5;
const ROUND_TRIP_COST_PCT = 0.001;

const DEFAULT_SEGMENTS = [
  ['w1', Date.parse('2026-06-01T00:00:00Z'), Date.parse('2026-06-09T00:00:00Z')],
  ['w2', Date.parse('2026-06-09T00:00:00Z'), Date.parse('2026-06-17T00:00:00Z')],
  ['w3', Date.parse('2026-06-17T00:00:00Z'), Date.parse('2026-06-25T00:00:00Z')],
  ['w4', Date.parse('2026-06-25T00:00:00Z'), Date.parse('2026-07-01T16:15:00Z') + 1],
];

export function equalTimeSegments(startMs, endMs, count = 4) {
  const span = endMs - startMs + 1;
  const width = Math.floor(span / count);
  return Array.from({ length: count }, (_, index) => {
    const start = startMs + width * index;
    const end = index === count - 1 ? endMs + 1 : startMs + width * (index + 1);
    return [`w${index + 1}`, start, end];
  });
}

const FILTER_GRID = {
  minImp: [2, 2.25, 2.5, 2.75],
  maxImp: [4, 4.5, 5, 5.5, 6],
  volN: [10, 20, 30],
  minVol: [1.1, 1.25, 1.4],
  minRet30: [2.5, 3, 3.5, 4],
  maxRet1h: [10, 12, 15],
  maxWick: [0.5, 0.7, 99],
  minM15: [-0.2, -0.1, 0],
  minM4h: [0, 0.3],
  minM24: [-8, -6],
  maxM24: [5, 8],
};

const EXTENDED_FILTER_GRID = {
  minImp: [2.25, 2.5, 2.75],
  maxImp: [4, 4.5, 5],
  volN: [10, 20],
  minVol: [1.1, 1.25, 1.4],
  maxVol: [999, 50, 25, 10],
  minRet30: [3, 3.5],
  maxRet1h: [10, 12, 15],
  maxWick: [0.5, 0.7],
  minM15: [-0.2, 0],
  minM4h: [0],
  minM24: [-8, -6],
  maxM24: [8],
  maxPrevRet5: [999, 2.5, 2, 1.5],
  minClosePosition: [0, 0.65, 0.8],
  minBodyRatio: [0, 0.4, 0.55],
};

const REFINE_FILTER_GRID = {
  minImp: [2.25, 2.5],
  maxImp: [4.25, 4.5],
  volN: [20],
  minVol: [1.1, 1.25],
  maxVol: [999, 50, 25],
  minRet30: [3, 3.25, 3.5],
  maxRet1h: [9, 10, 11],
  maxWick: [0.45, 0.5, 0.6],
  minM15: [-0.2, -0.1],
  minM4h: [0],
  minM24: [-8, -6],
  maxM24: [8],
  maxPrevRet5: [999, 3, 2.5, 2],
  minClosePosition: [0.78, 0.8, 0.82, 0.85, 0.88],
  minBodyRatio: [0, 0.25, 0.4],
};

const STABLE45_FILTER_GRID = {
  minImp: [2, 2.25, 2.5],
  maxImp: [3.75, 4, 4.25, 4.5],
  volN: [20, 30],
  minVol: [1.1, 1.25, 1.4, 1.55],
  maxVol: [999],
  minRet30: [3.25, 3.5, 3.75, 4],
  maxRet1h: [12, 15, 18],
  maxWick: [0.5, 0.7, 99],
  minM15: [-0.2, -0.1, 0],
  minM4h: [0],
  minM24: [-8, -6],
  maxM24: [5, 8],
  maxPrevRet5: [999],
  minClosePosition: [0, 0.65, 0.8],
  minBodyRatio: [0],
};

const RISK_GRID = {
  stopPct: [0.025, 0.03, 0.0325, 0.035, 0.0375, 0.04, 0.045, 0.05],
  targetR: [1, 1.1, 1.2, 1.3, 1.5, 1.7, 2],
  cooldownMinutes: [60, 90, 120, 180, 240, 360],
};

const REFINED_RISK_GRID = {
  stopPct: [0.035, 0.036, 0.037, 0.0375, 0.038, 0.039, 0.04],
  targetR: [1.05, 1.1, 1.15, 1.2],
  cooldownMinutes: [60, 90, 120],
};

const STABLE45_RISK_GRID = {
  stopPct: [0.0325, 0.035, 0.0375, 0.04],
  targetR: [1, 1.05, 1.1, 1.15, 1.2, 1.3],
  cooldownMinutes: [60, 90, 120, 180],
};

const SYMBOL_CAP_RISK_GRID = {
  stopPct: [0.0325, 0.035, 0.0375, 0.04],
  targetR: [1.1, 1.2, 1.3, 1.5],
  cooldownMinutes: [60, 90, 120, 180, 240, 360],
  maxTradesPerSymbolPerSegment: [2, 3, 4, 5, 6],
};

const BASELINE_FILTER = {
  minImp: 2.25,
  maxImp: 5,
  volN: 20,
  minVol: 1.25,
  maxVol: 999,
  minRet30: 3,
  maxRet1h: 12,
  maxWick: 99,
  minM15: -0.1,
  minM4h: 0,
  minM24: -8,
  maxM24: 5,
};

const BASELINE_RISK = {
  stopPct: 0.035,
  targetR: 1.2,
  cooldownMinutes: 120,
};

function round(value, digits = 3) {
  const scale = 10 ** digits;
  return Math.round((value + Number.EPSILON) * scale) / scale;
}

function pct(from, to) {
  return (to / from - 1) * 100;
}

function averageVolume(rows, start, end) {
  let total = 0;
  let count = 0;
  for (let index = start; index < end; index += 1) {
    if (index >= 0 && index < rows.length) {
      total += rows[index].vol;
      count += 1;
    }
  }
  return count === 0 ? Number.NaN : total / count;
}

function upperWickRatio(candle) {
  const range = candle.high - candle.low;
  if (range <= 0) {
    return 0;
  }
  return (candle.high - Math.max(candle.open, candle.close)) / range;
}

function closePosition(candle) {
  const range = candle.high - candle.low;
  if (range <= 0) {
    return 0.5;
  }
  return (candle.close - candle.low) / range;
}

function bodyRatio(candle) {
  const range = candle.high - candle.low;
  if (range <= 0) {
    return 0;
  }
  return Math.abs(candle.close - candle.open) / range;
}

function symbolTotals(trades) {
  const totals = new Map();
  for (const trade of trades) {
    totals.set(trade.symbol, (totals.get(trade.symbol) ?? 0) + trade.r);
  }
  return [...totals.entries()].sort((left, right) => right[1] - left[1]);
}

/** Return total R after removing the highest-contributing symbols. */
export function removeTopSymbolTotal(trades, removeTopCount) {
  const removedSymbols = new Set(
    symbolTotals(trades)
      .slice(0, removeTopCount)
      .map(([symbol]) => symbol),
  );
  return trades
    .filter((trade) => !removedSymbols.has(trade.symbol))
    .reduce((total, trade) => total + trade.r, 0);
}

/** Limit repeated exposure to one symbol inside each time window. */
export function capTradesPerSymbolBySegment(trades, segments, maxTrades) {
  if (!Number.isFinite(maxTrades) || maxTrades <= 0) {
    return [...trades].sort((left, right) => left.entryTs - right.entryTs);
  }
  const counts = new Map();
  const capped = [];
  for (const trade of [...trades].sort((left, right) => left.entryTs - right.entryTs)) {
    const segment = segments.find(([, start, end]) => trade.entryTs >= start && trade.entryTs < end);
    const segmentLabel = segment?.[0] ?? 'outside';
    const key = `${segmentLabel}:${trade.symbol}`;
    const count = counts.get(key) ?? 0;
    if (count >= maxTrades) {
      continue;
    }
    counts.set(key, count + 1);
    capped.push(trade);
  }
  return capped;
}

/** Build a compact equity summary for a trade list. */
export function summarizeTrades(trades) {
  const sortedTrades = [...trades].sort((left, right) => left.entryTs - right.entryTs);
  let equity = 0;
  let peak = 0;
  let maxDrawdownR = 0;
  let wins = 0;
  for (const trade of sortedTrades) {
    equity += trade.r;
    if (trade.r > 0) {
      wins += 1;
    }
    peak = Math.max(peak, equity);
    maxDrawdownR = Math.max(maxDrawdownR, peak - equity);
  }
  return {
    tradeCount: sortedTrades.length,
    winRatePct: sortedTrades.length === 0 ? 0 : (wins / sortedTrades.length) * 100,
    totalR: equity,
    maxDrawdownR,
    removeTop3R: removeTopSymbolTotal(sortedTrades, 3),
    removeTop5R: removeTopSymbolTotal(sortedTrades, 5),
    topSymbols: symbolTotals(sortedTrades).slice(0, 5),
  };
}

/** Summarize fixed time windows and their local concentration. */
export function segmentReports(trades, segments, removeTopCount = 3) {
  return segments.map(([label, start, end]) => {
    const segmentTrades = trades.filter((trade) => trade.entryTs >= start && trade.entryTs < end);
    const summary = summarizeTrades(segmentTrades);
    return {
      label,
      tradeCount: summary.tradeCount,
      winRatePct: summary.winRatePct,
      totalR: summary.totalR,
      removeTopR: removeTopSymbolTotal(segmentTrades, removeTopCount),
    };
  });
}

function buildMacroMap(macroCache) {
  const symbols = Object.keys(macroCache.candlesBySymbol);
  const firstRows = macroCache.candlesBySymbol[symbols[0]] ?? [];
  const macroMap = new Map();
  for (let index = 288; index < firstRows.length; index += 1) {
    const values = symbols.map((symbol) => {
      const rows = macroCache.candlesBySymbol[symbol];
      return {
        ret15m: pct(rows[index - 3].close, rows[index].close),
        ret4h: pct(rows[index - 48].close, rows[index].close),
        ret24h: pct(rows[index - 288].close, rows[index].close),
      };
    });
    macroMap.set(firstRows[index].ts, {
      min15m: Math.min(...values.map((value) => value.ret15m)),
      min4h: Math.min(...values.map((value) => value.ret4h)),
      min24h: Math.min(...values.map((value) => value.ret24h)),
      max24h: Math.max(...values.map((value) => value.ret24h)),
    });
  }
  return macroMap;
}

function tradeOutcome(rows, entryIndex, stopPct, targetR) {
  const entry = rows[entryIndex].open;
  const stop = entry * (1 - stopPct);
  const target = entry * (1 + stopPct * targetR);
  let grossR = (rows[entryIndex + HOLDING_CANDLES].close - entry) / (entry * stopPct);
  for (let index = entryIndex; index <= entryIndex + HOLDING_CANDLES; index += 1) {
    const candle = rows[index];
    if (candle.low <= stop) {
      grossR = -1;
      break;
    }
    if (candle.high >= target) {
      grossR = targetR;
      break;
    }
  }
  return grossR - ROUND_TRIP_COST_PCT / stopPct;
}

function buildFeatures(rankCache, macroCache, riskPairs) {
  const macroMap = buildMacroMap(macroCache);
  const bySymbol = new Map();
  let featureCount = 0;
  for (const [symbol, rows] of Object.entries(rankCache.candlesBySymbol)) {
    const features = [];
    for (let index = 288; index < rows.length - 1 - HOLDING_CANDLES; index += 1) {
      const candle = rows[index];
      const ret5 = pct(rows[index - 1].close, candle.close);
      const ret30 = pct(rows[index - 6].close, candle.close);
      const ret1h = pct(rows[index - 12].close, candle.close);
      if (ret5 < 1.5 || ret5 > 8 || ret30 < 1.5 || ret1h > 25) {
        continue;
      }
      const macro = macroMap.get(candle.ts);
      if (!macro) {
        continue;
      }
      const entryIndex = index + 1;
      const outcomes = new Map();
      for (const [stopPct, targetR] of riskPairs) {
        outcomes.set(riskKey(stopPct, targetR), tradeOutcome(rows, entryIndex, stopPct, targetR));
      }
      features.push({
        symbol,
        entryTs: rows[entryIndex].ts,
        ret5,
        prevRet5: pct(rows[index - 2].close, rows[index - 1].close),
        ret30,
        ret1h,
        v10: candle.vol / averageVolume(rows, index - 10, index),
        v20: candle.vol / averageVolume(rows, index - 20, index),
        v30: candle.vol / averageVolume(rows, index - 30, index),
        upperWick: upperWickRatio(candle),
        closePosition: closePosition(candle),
        bodyRatio: bodyRatio(candle),
        min15m: macro.min15m,
        min4h: macro.min4h,
        min24h: macro.min24h,
        max24h: macro.max24h,
        outcomes,
      });
      featureCount += 1;
    }
    bySymbol.set(symbol, features);
  }
  return { bySymbol, featureCount };
}

function riskKey(stopPct, targetR) {
  return `${stopPct}:${targetR}`;
}

export function filterMatches(feature, filter) {
  const volumeRatio = feature[`v${filter.volN}`];
  return (
    feature.ret5 >= filter.minImp &&
    feature.ret5 <= filter.maxImp &&
    volumeRatio >= filter.minVol &&
    volumeRatio <= (filter.maxVol ?? 999) &&
    feature.ret30 >= filter.minRet30 &&
    feature.ret1h <= filter.maxRet1h &&
    feature.upperWick <= filter.maxWick &&
    feature.min15m >= filter.minM15 &&
    feature.min4h >= filter.minM4h &&
    feature.min24h >= filter.minM24 &&
    feature.max24h <= filter.maxM24 &&
    feature.prevRet5 <= (filter.maxPrevRet5 ?? 999) &&
    feature.closePosition >= (filter.minClosePosition ?? 0) &&
    feature.bodyRatio >= (filter.minBodyRatio ?? 0)
  );
}

export function evaluateConfig(featureSet, filter, risk, segments = DEFAULT_SEGMENTS) {
  const trades = [];
  const outcomeKey = riskKey(risk.stopPct, risk.targetR);
  for (const [symbol, features] of featureSet.bySymbol) {
    let nextAllowedAt = Number.NEGATIVE_INFINITY;
    for (const feature of features) {
      if (feature.entryTs < nextAllowedAt || !filterMatches(feature, filter)) {
        continue;
      }
      trades.push({
        symbol,
        entryTs: feature.entryTs,
        r: feature.outcomes.get(outcomeKey),
      });
      nextAllowedAt = feature.entryTs + risk.cooldownMinutes * 60 * 1000;
    }
  }
  const selectedTrades = risk.maxTradesPerSymbolPerSegment
    ? capTradesPerSymbolBySegment(trades, segments, risk.maxTradesPerSymbolPerSegment)
    : trades;
  const summary = summarizeTrades(selectedTrades);
  const segmentSummaries = segmentReports(selectedTrades, segments, 3);
  return {
    filter,
    risk,
    summary,
    segments: segmentSummaries,
    minSegmentR: Math.min(...segmentSummaries.map((segment) => segment.totalR)),
    minSegmentRemoveTopR: Math.min(...segmentSummaries.map((segment) => segment.removeTopR)),
  };
}

function compactResult(result) {
  return {
    score: round(result.score),
    n: result.summary.tradeCount,
    win: round(result.summary.winRatePct, 2),
    totalR: round(result.summary.totalR),
    maxDdR: round(result.summary.maxDrawdownR),
    removeTop3R: round(result.summary.removeTop3R),
    removeTop5R: round(result.summary.removeTop5R),
    minSegmentR: round(result.minSegmentR),
    minSegmentRemoveTopR: round(result.minSegmentRemoveTopR),
    filter: result.filter,
    risk: result.risk,
    segments: result.segments.map((segment) => ({
      label: segment.label,
      n: segment.tradeCount,
      win: round(segment.winRatePct, 1),
      totalR: round(segment.totalR),
      removeTopR: round(segment.removeTopR),
    })),
    topSymbols: result.summary.topSymbols.map(([symbol, total]) => [symbol, round(total)]),
  };
}

/** Build the reproducible research contract for the best 5m impulse candidate. */
export function buildResearchPresetContract(candidate, sample) {
  return {
    schemaVersion: 1,
    strategyKey: 'market_velocity_5m_direct_impulse',
    version: 'research_mv5m_direct_impulse_0375sl_11r_v1',
    status: 'research_only',
    timeframe: '5m',
    universe: 'market_velocity_rank_hot_top30',
    direction: 'long',
    entry: 'next_5m_open_after_signal_close',
    filter: { ...candidate.filter },
    risk: { ...candidate.risk },
    evidence: {
      sampleStart: sample.start,
      sampleEnd: sample.end,
      metrics: {
        n: candidate.n,
        win: candidate.win,
        totalR: candidate.totalR,
        maxDdR: candidate.maxDdR,
        removeTop5R: candidate.removeTop5R,
        minSegmentRemoveTopR: candidate.minSegmentRemoveTopR,
      },
    },
    promotion: {
      liveEligible: false,
      requiredNextStep: 'paper_observation',
      reason: '5m direct impulse is not wired into the 15m production preset path.',
    },
  };
}

function pushTop(results, result, limit, score) {
  result.score = score;
  results.push(result);
  results.sort((left, right) => right.score - left.score);
  if (results.length > limit) {
    results.pop();
  }
}

/** Score candidates for the current bottleneck: recent-window concentration. */
export function recentStableScore(result, recentSegmentLabel = 'w4') {
  const recentSegment = result.segments.find((segment) => segment.label === recentSegmentLabel);
  const recentRemoveTopR = recentSegment?.removeTopR ?? Number.NEGATIVE_INFINITY;
  const recentTotalR = recentSegment?.totalR ?? Number.NEGATIVE_INFINITY;
  return (
    recentRemoveTopR * 6 +
    recentTotalR * 2 +
    result.summary.totalR +
    result.summary.removeTop5R * 0.7 -
    result.summary.maxDrawdownR +
    Math.min(0, result.minSegmentRemoveTopR ?? recentRemoveTopR) * 2 +
    (result.summary.winRatePct - 60) * 0.1
  );
}

/** Apply the current recent-window stability gate used by the research scan. */
export function isRecentStableCandidate(result, recentSegmentLabel = 'w4') {
  const summary = result.summary;
  const recentSegment = result.segments.find((segment) => segment.label === recentSegmentLabel);
  return (
    summary.tradeCount >= 45 &&
    summary.winRatePct >= 58 &&
    summary.totalR >= 8 &&
    summary.maxDrawdownR <= 8 &&
    recentSegment &&
    recentSegment.tradeCount >= 5 &&
    recentSegment.totalR >= 0 &&
    recentSegment.removeTopR >= 0
  );
}

/** Require every segment to stay non-negative after top-contributor removal. */
export function isAllSegmentStableCandidate(result) {
  const summary = result.summary;
  return (
    summary.tradeCount >= 40 &&
    summary.winRatePct >= 58 &&
    summary.totalR >= 8 &&
    summary.maxDrawdownR <= 8 &&
    result.segments.every(
      (segment) =>
        segment.tradeCount >= 5 && segment.totalR >= 0 && segment.removeTopR >= 0,
    )
  );
}

/** Score candidates by worst-window concentration before raw return. */
export function robustnessScore(result) {
  return (
    result.minSegmentRemoveTopR * 12 +
    result.summary.totalR +
    result.summary.removeTop5R * 0.5 -
    result.summary.maxDrawdownR +
    (result.summary.winRatePct - 60) * 0.1
  );
}

function symbolCapScore(result) {
  return (
    result.minSegmentRemoveTopR * 8 +
    result.summary.removeTop5R * 1.2 +
    result.summary.totalR -
    result.summary.maxDrawdownR +
    (result.summary.winRatePct - 60) * 0.1
  );
}

function scanFilters(featureSet, segments = DEFAULT_SEGMENTS) {
  const top = [];
  let checked = 0;
  let matched = 0;
  for (const minImp of FILTER_GRID.minImp) {
    for (const maxImp of FILTER_GRID.maxImp) {
      if (maxImp <= minImp + 0.75) {
        continue;
      }
      for (const volN of FILTER_GRID.volN) {
        for (const minVol of FILTER_GRID.minVol) {
          for (const minRet30 of FILTER_GRID.minRet30) {
            for (const maxRet1h of FILTER_GRID.maxRet1h) {
              for (const maxWick of FILTER_GRID.maxWick) {
                for (const minM15 of FILTER_GRID.minM15) {
                  for (const minM4h of FILTER_GRID.minM4h) {
                    for (const minM24 of FILTER_GRID.minM24) {
                      for (const maxM24 of FILTER_GRID.maxM24) {
                        checked += 1;
                        const filter = {
                          minImp,
                          maxImp,
                          volN,
                          minVol,
                          maxVol: 999,
                          minRet30,
                          maxRet1h,
                          maxWick,
                          minM15,
                          minM4h,
                          minM24,
                          maxM24,
                        };
                        const result = evaluateConfig(featureSet, filter, BASELINE_RISK, segments);
                        const summary = result.summary;
                        if (
                          summary.tradeCount < 45 ||
                          summary.winRatePct < 58 ||
                          summary.totalR < 8 ||
                          summary.maxDrawdownR > 8
                        ) {
                          continue;
                        }
                        matched += 1;
                        const score =
                          summary.totalR +
                          summary.removeTop5R * 0.9 -
                          summary.maxDrawdownR * 0.8 +
                          Math.min(0, result.minSegmentR) * 3 +
                          Math.min(0, result.minSegmentRemoveTopR) * 2 +
                          (summary.winRatePct - 60) * 0.1;
                        pushTop(top, result, 20, score);
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  return { checked, matched, top };
}

function scanRobustness(featureSet, segments = DEFAULT_SEGMENTS) {
  const top = [];
  let checked = 0;
  let matched = 0;
  for (const minImp of FILTER_GRID.minImp) {
    for (const maxImp of FILTER_GRID.maxImp) {
      if (maxImp <= minImp + 0.75) {
        continue;
      }
      for (const volN of FILTER_GRID.volN) {
        for (const minVol of FILTER_GRID.minVol) {
          for (const minRet30 of FILTER_GRID.minRet30) {
            for (const maxRet1h of FILTER_GRID.maxRet1h) {
              for (const maxWick of FILTER_GRID.maxWick) {
                for (const minM15 of FILTER_GRID.minM15) {
                  for (const minM4h of FILTER_GRID.minM4h) {
                    for (const minM24 of FILTER_GRID.minM24) {
                      for (const maxM24 of FILTER_GRID.maxM24) {
                        checked += 1;
                        const filter = {
                          minImp,
                          maxImp,
                          volN,
                          minVol,
                          maxVol: 999,
                          minRet30,
                          maxRet1h,
                          maxWick,
                          minM15,
                          minM4h,
                          minM24,
                          maxM24,
                        };
                        const result = evaluateConfig(featureSet, filter, BASELINE_RISK, segments);
                        const summary = result.summary;
                        if (
                          summary.tradeCount < 40 ||
                          summary.winRatePct < 58 ||
                          summary.totalR < 8 ||
                          summary.maxDrawdownR > 8
                        ) {
                          continue;
                        }
                        matched += 1;
                        pushTop(top, result, 20, robustnessScore(result));
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  return { checked, matched, top };
}

function* gridFilters(grid) {
  const keys = Object.keys(grid);
  function* walk(index, current) {
    if (index === keys.length) {
      yield current;
      return;
    }
    const key = keys[index];
    for (const value of grid[key]) {
      yield* walk(index + 1, { ...current, [key]: value });
    }
  }
  yield* walk(0, {});
}

function scanFeatureGrid(featureSet, grid, segments = DEFAULT_SEGMENTS) {
  const top = [];
  let checked = 0;
  let matched = 0;
  for (const filter of gridFilters(grid)) {
    if (filter.maxImp <= filter.minImp + 0.75) {
      continue;
    }
    checked += 1;
    const result = evaluateConfig(featureSet, filter, BASELINE_RISK, segments);
    const summary = result.summary;
    if (
      summary.tradeCount < 40 ||
      summary.winRatePct < 58 ||
      summary.totalR < 8 ||
      summary.maxDrawdownR > 8
    ) {
      continue;
    }
    matched += 1;
    const score =
      summary.totalR +
      summary.removeTop5R * 0.6 -
      summary.maxDrawdownR +
      result.minSegmentRemoveTopR * 8 +
      Math.min(0, result.minSegmentR) * 2 +
      (summary.winRatePct - 60) * 0.1;
    pushTop(top, result, 20, score);
  }
  return { checked, matched, top };
}

function scanExtendedFeatures(featureSet, segments = DEFAULT_SEGMENTS) {
  return scanFeatureGrid(featureSet, EXTENDED_FILTER_GRID, segments);
}

function scanRefinedFeatures(featureSet, segments = DEFAULT_SEGMENTS) {
  return scanFeatureGrid(featureSet, REFINE_FILTER_GRID, segments);
}

function scanAllSegmentStable(featureSet, segments = DEFAULT_SEGMENTS) {
  const top = [];
  let checked = 0;
  let matched = 0;
  for (const minImp of FILTER_GRID.minImp) {
    for (const maxImp of FILTER_GRID.maxImp) {
      if (maxImp <= minImp + 0.75) {
        continue;
      }
      for (const volN of FILTER_GRID.volN) {
        for (const minVol of FILTER_GRID.minVol) {
          for (const minRet30 of FILTER_GRID.minRet30) {
            for (const maxRet1h of FILTER_GRID.maxRet1h) {
              for (const maxWick of FILTER_GRID.maxWick) {
                for (const minM15 of FILTER_GRID.minM15) {
                  for (const minM4h of FILTER_GRID.minM4h) {
                    for (const minM24 of FILTER_GRID.minM24) {
                      for (const maxM24 of FILTER_GRID.maxM24) {
                        checked += 1;
                        const filter = {
                          minImp,
                          maxImp,
                          volN,
                          minVol,
                          maxVol: 999,
                          minRet30,
                          maxRet1h,
                          maxWick,
                          minM15,
                          minM4h,
                          minM24,
                          maxM24,
                        };
                        const result = evaluateConfig(featureSet, filter, BASELINE_RISK, segments);
                        if (!isAllSegmentStableCandidate(result)) {
                          continue;
                        }
                        matched += 1;
                        const score =
                          result.summary.totalR +
                          result.summary.removeTop5R -
                          result.summary.maxDrawdownR +
                          result.minSegmentRemoveTopR * 3 +
                          (result.summary.winRatePct - 60) * 0.1;
                        pushTop(top, result, 20, score);
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  return { checked, matched, top };
}

function scanRecentStable(featureSet, segments = DEFAULT_SEGMENTS) {
  const top = [];
  let checked = 0;
  let matched = 0;
  for (const minImp of FILTER_GRID.minImp) {
    for (const maxImp of FILTER_GRID.maxImp) {
      if (maxImp <= minImp + 0.75) {
        continue;
      }
      for (const volN of FILTER_GRID.volN) {
        for (const minVol of FILTER_GRID.minVol) {
          for (const minRet30 of FILTER_GRID.minRet30) {
            for (const maxRet1h of FILTER_GRID.maxRet1h) {
              for (const maxWick of FILTER_GRID.maxWick) {
                for (const minM15 of FILTER_GRID.minM15) {
                  for (const minM4h of FILTER_GRID.minM4h) {
                    for (const minM24 of FILTER_GRID.minM24) {
                      for (const maxM24 of FILTER_GRID.maxM24) {
                        checked += 1;
                        const filter = {
                          minImp,
                          maxImp,
                          volN,
                          minVol,
                          maxVol: 999,
                          minRet30,
                          maxRet1h,
                          maxWick,
                          minM15,
                          minM4h,
                          minM24,
                          maxM24,
                        };
                        const result = evaluateConfig(featureSet, filter, BASELINE_RISK, segments);
                        if (!isRecentStableCandidate(result)) {
                          continue;
                        }
                        matched += 1;
                        pushTop(top, result, 20, recentStableScore(result));
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  return { checked, matched, top };
}

function scanRisk(featureSet, filter, segments = DEFAULT_SEGMENTS) {
  const top = [];
  let checked = 0;
  let matched = 0;
  for (const stopPct of RISK_GRID.stopPct) {
    for (const targetR of RISK_GRID.targetR) {
      for (const cooldownMinutes of RISK_GRID.cooldownMinutes) {
        checked += 1;
        const risk = { stopPct, targetR, cooldownMinutes };
        const result = evaluateConfig(featureSet, filter, risk, segments);
        const summary = result.summary;
        if (
          summary.tradeCount < 45 ||
          summary.winRatePct < 58 ||
          summary.totalR < 8 ||
          summary.maxDrawdownR > 8
        ) {
          continue;
        }
        matched += 1;
        const score =
          summary.totalR +
          summary.removeTop5R * 0.9 -
          summary.maxDrawdownR * 0.9 +
          Math.min(0, result.minSegmentRemoveTopR) * 2 +
          (summary.winRatePct - 60) * 0.1;
        pushTop(top, result, 20, score);
      }
    }
  }
  return { checked, matched, top };
}

function scanRiskForFilters(
  featureSet,
  filters,
  predicate,
  scoreResult,
  riskGrid = RISK_GRID,
  segments = DEFAULT_SEGMENTS,
) {
  const top = [];
  let checked = 0;
  let matched = 0;
  const uniqueFilters = [...new Map(filters.map((filter) => [JSON.stringify(filter), filter])).values()];
  for (const filter of uniqueFilters) {
    for (const stopPct of riskGrid.stopPct) {
      for (const targetR of riskGrid.targetR) {
        for (const cooldownMinutes of riskGrid.cooldownMinutes) {
          checked += 1;
          for (const maxTradesPerSymbolPerSegment of riskGrid.maxTradesPerSymbolPerSegment ?? [undefined]) {
            const risk = { stopPct, targetR, cooldownMinutes };
            if (maxTradesPerSymbolPerSegment !== undefined) {
              risk.maxTradesPerSymbolPerSegment = maxTradesPerSymbolPerSegment;
            }
            const result = evaluateConfig(featureSet, filter, risk, segments);
            if (!predicate(result)) {
              continue;
            }
            matched += 1;
            pushTop(top, result, 20, scoreResult(result));
          }
        }
      }
    }
  }
  return { checked, matched, top };
}

export function parseArgs(argv) {
  const args = {
    rankCache: DEFAULT_RANK_CACHE,
    macroCache: DEFAULT_MACRO_CACHE,
    mode: 'standard',
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === '--rank-cache') {
      args.rankCache = argv[++index];
    } else if (flag === '--macro-cache') {
      args.macroCache = argv[++index];
    } else if (flag === '--mode') {
      args.mode = argv[++index];
    } else if (flag === '--help') {
      args.help = true;
    } else {
      throw new Error(`unknown argument: ${flag}`);
    }
  }
  if (!['standard', 'extended', 'refine', 'stable45', 'symbolcap', 'all', 'baseline'].includes(args.mode)) {
    throw new Error(`unknown --mode: ${args.mode}`);
  }
  return args;
}

function printUsage() {
  console.log(`Usage: node scripts/research/market_velocity_5m_impulse_scan.mjs [--mode baseline|standard|extended|refine|stable45|symbolcap|all] [--rank-cache PATH] [--macro-cache PATH]

Scans the local 5m Market Velocity impulse research cache. The tool is read-only
and does not connect to production or submit orders.`);
}

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, 'utf8'));
}

function allRiskPairs() {
  const pairs = [[BASELINE_RISK.stopPct, BASELINE_RISK.targetR]];
  for (const stopPct of RISK_GRID.stopPct) {
    for (const targetR of RISK_GRID.targetR) {
      pairs.push([stopPct, targetR]);
    }
  }
  return [...new Map(pairs.map((pair) => [riskKey(pair[0], pair[1]), pair])).values()];
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printUsage();
    return;
  }
  const rankCache = readJson(args.rankCache);
  const macroCache = readJson(args.macroCache);
  const featureSet = buildFeatures(rankCache, macroCache, allRiskPairs());
  const segments = equalTimeSegments(rankCache.start, rankCache.end, 4);
  const baseline = evaluateConfig(featureSet, BASELINE_FILTER, BASELINE_RISK, segments);
  const output = {
    caches: {
      rankCache: args.rankCache,
      macroCache: args.macroCache,
      start: new Date(rankCache.start).toISOString(),
      end: new Date(rankCache.end).toISOString(),
    },
    segments: segments.map(([label, start, end]) => ({
      label,
      start: new Date(start).toISOString(),
      end: new Date(end - 1).toISOString(),
    })),
    featureCount: featureSet.featureCount,
    baseline: compactResult({ ...baseline, score: 0 }),
  };
  if (args.mode === 'standard' || args.mode === 'all') {
    const filterScan = scanFilters(featureSet, segments);
    const robustnessScan = scanRobustness(featureSet, segments);
    const allSegmentStableScan = scanAllSegmentStable(featureSet, segments);
    const recentStableScan = scanRecentStable(featureSet, segments);
    const bestFilter = filterScan.top[0]?.filter ?? BASELINE_FILTER;
    const riskScan = scanRisk(featureSet, bestFilter, segments);
    const stableRiskScan = scanRiskForFilters(
      featureSet,
      [
        bestFilter,
        ...recentStableScan.top.slice(0, 10).map((result) => result.filter),
      ],
      isAllSegmentStableCandidate,
      (result) =>
      result.summary.totalR +
      result.summary.removeTop5R -
      result.summary.maxDrawdownR +
      result.minSegmentRemoveTopR * 3 +
      (result.summary.winRatePct - 60) * 0.1,
      REFINED_RISK_GRID,
      segments,
    );
    output.filterScan = {
      checked: filterScan.checked,
      matched: filterScan.matched,
      top: filterScan.top.slice(0, 10).map(compactResult),
    };
    output.robustnessScan = {
      checked: robustnessScan.checked,
      matched: robustnessScan.matched,
      top: robustnessScan.top.slice(0, 10).map(compactResult),
    };
    output.allSegmentStableScan = {
      checked: allSegmentStableScan.checked,
      matched: allSegmentStableScan.matched,
      top: allSegmentStableScan.top.slice(0, 10).map(compactResult),
    };
    output.recentStableScan = {
      checked: recentStableScan.checked,
      matched: recentStableScan.matched,
      top: recentStableScan.top.slice(0, 10).map(compactResult),
    };
    output.riskScan = {
      checked: riskScan.checked,
      matched: riskScan.matched,
      top: riskScan.top.slice(0, 10).map(compactResult),
    };
    output.stableRiskScan = {
      checked: stableRiskScan.checked,
      matched: stableRiskScan.matched,
      top: stableRiskScan.top.slice(0, 10).map(compactResult),
    };
  }
  if (args.mode === 'extended' || args.mode === 'all') {
    const extendedFeatureScan = scanExtendedFeatures(featureSet, segments);
    output.extendedFeatureScan = {
      checked: extendedFeatureScan.checked,
      matched: extendedFeatureScan.matched,
      top: extendedFeatureScan.top.slice(0, 10).map(compactResult),
    };
  }
  if (args.mode === 'stable45' || args.mode === 'all') {
    const stable45FeatureScan = scanFeatureGrid(featureSet, STABLE45_FILTER_GRID, segments);
    const stable45RiskScan = scanRiskForFilters(
      featureSet,
      stable45FeatureScan.top.slice(0, 20).map((result) => result.filter),
      isAllSegmentStableCandidate,
      (result) =>
        result.summary.totalR +
        result.summary.removeTop5R -
        result.summary.maxDrawdownR +
        result.minSegmentRemoveTopR * 3 +
        (result.summary.winRatePct - 60) * 0.1,
      STABLE45_RISK_GRID,
      segments,
    );
    output.stable45FeatureScan = {
      checked: stable45FeatureScan.checked,
      matched: stable45FeatureScan.matched,
      top: stable45FeatureScan.top.slice(0, 10).map(compactResult),
    };
    output.stable45RiskScan = {
      checked: stable45RiskScan.checked,
      matched: stable45RiskScan.matched,
      top: stable45RiskScan.top.slice(0, 10).map(compactResult),
    };
  }
  if (args.mode === 'refine' || args.mode === 'all') {
    const refinedFeatureScan = scanRefinedFeatures(featureSet, segments);
    const refinedRiskScan = scanRiskForFilters(
      featureSet,
      refinedFeatureScan.top.slice(0, 10).map((result) => result.filter),
      isAllSegmentStableCandidate,
      (result) =>
        result.summary.totalR +
        result.summary.removeTop5R -
        result.summary.maxDrawdownR +
        result.minSegmentRemoveTopR * 3 +
        (result.summary.winRatePct - 60) * 0.1,
      RISK_GRID,
      segments,
    );
    const refinedRiskTop = refinedRiskScan.top.slice(0, 10).map(compactResult);
    output.refinedFeatureScan = {
      checked: refinedFeatureScan.checked,
      matched: refinedFeatureScan.matched,
      top: refinedFeatureScan.top.slice(0, 10).map(compactResult),
    };
    output.refinedRiskScan = {
      checked: refinedRiskScan.checked,
      matched: refinedRiskScan.matched,
      top: refinedRiskTop,
    };
    if (refinedRiskTop[0]) {
      output.researchPresetContract = buildResearchPresetContract(refinedRiskTop[0], output.caches);
    }
  }
  if (args.mode === 'symbolcap') {
    const filterScan = scanFilters(featureSet, segments);
    const extendedFeatureScan = scanExtendedFeatures(featureSet, segments);
    const stable45FeatureScan = scanFeatureGrid(featureSet, STABLE45_FILTER_GRID, segments);
    const refinedFeatureScan = scanRefinedFeatures(featureSet, segments);
    const sourceFilters = [
      ...filterScan.top.slice(0, 10).map((result) => result.filter),
      ...extendedFeatureScan.top.slice(0, 10).map((result) => result.filter),
      ...stable45FeatureScan.top.slice(0, 10).map((result) => result.filter),
      ...refinedFeatureScan.top.slice(0, 10).map((result) => result.filter),
    ];
    const symbolCapRiskScan = scanRiskForFilters(
      featureSet,
      sourceFilters,
      isAllSegmentStableCandidate,
      symbolCapScore,
      SYMBOL_CAP_RISK_GRID,
      segments,
    );
    output.symbolCapSources = {
      filterMatched: filterScan.matched,
      extendedMatched: extendedFeatureScan.matched,
      stable45Matched: stable45FeatureScan.matched,
      refinedMatched: refinedFeatureScan.matched,
    };
    output.symbolCapRiskScan = {
      checked: symbolCapRiskScan.checked,
      matched: symbolCapRiskScan.matched,
      top: symbolCapRiskScan.top.slice(0, 10).map(compactResult),
    };
  }
  console.log(JSON.stringify(output, null, 2));
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
