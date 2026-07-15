import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  buildResearchPresetContract,
  capTradesPerSymbolBySegment,
  evaluateConfig,
  equalTimeSegments,
  filterMatches,
  isAllSegmentStableCandidate,
  isRecentStableCandidate,
  parseArgs,
  removeTopSymbolTotal,
  recentStableScore,
  robustnessScore,
  segmentReports,
  summarizeTrades,
} from './market_velocity_5m_impulse_scan.mjs';

function assertNear(actual, expected) {
  assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} should be near ${expected}`);
}

test('summarizeTrades calculates win rate, drawdown, and symbol concentration', () => {
  const trades = [
    { symbol: 'AAA', entryTs: 1, r: 1.2 },
    { symbol: 'BBB', entryTs: 2, r: -1.0 },
    { symbol: 'AAA', entryTs: 3, r: 1.2 },
    { symbol: 'CCC', entryTs: 4, r: -1.0 },
  ];

  const summary = summarizeTrades(trades);

  assert.equal(summary.tradeCount, 4);
  assert.equal(summary.winRatePct, 50);
  assertNear(summary.totalR, 0.4);
  assert.equal(summary.maxDrawdownR, 1);
  assert.equal(removeTopSymbolTotal(trades, 1), -2);
});

test('segmentReports computes per-window remove-top concentration', () => {
  const trades = [
    { symbol: 'AAA', entryTs: 10, r: 1.2 },
    { symbol: 'BBB', entryTs: 20, r: -1.0 },
    { symbol: 'CCC', entryTs: 30, r: 1.2 },
    { symbol: 'AAA', entryTs: 110, r: -1.0 },
  ];
  const segments = [
    ['early', 0, 100],
    ['late', 100, 200],
  ];

  const reports = segmentReports(trades, segments, 1);

  assert.equal(reports[0].label, 'early');
  assert.equal(reports[0].tradeCount, 3);
  assertNear(reports[0].totalR, 1.4);
  assertNear(reports[0].removeTopR, 0.2);
  assert.equal(reports[1].label, 'late');
  assert.equal(reports[1].tradeCount, 1);
  assert.equal(reports[1].totalR, -1);
  assert.equal(reports[1].removeTopR, 0);
});

test('capTradesPerSymbolBySegment keeps only the first trades per symbol in each segment', () => {
  const trades = [
    { symbol: 'AAA', entryTs: 10, r: 1 },
    { symbol: 'AAA', entryTs: 20, r: 1 },
    { symbol: 'BBB', entryTs: 30, r: 1 },
    { symbol: 'AAA', entryTs: 110, r: 1 },
    { symbol: 'AAA', entryTs: 120, r: 1 },
  ];
  const segments = [
    ['w1', 0, 100],
    ['w2', 100, 200],
  ];

  const capped = capTradesPerSymbolBySegment(trades, segments, 1);

  assert.deepEqual(capped.map((trade) => `${trade.symbol}:${trade.entryTs}`), [
    'AAA:10',
    'BBB:30',
    'AAA:110',
  ]);
});

test('evaluateConfig applies optional per-symbol segment cap from risk config', () => {
  const matchingFeature = (symbol, entryTs, r) => ({
    symbol,
    entryTs,
    ret5: 3,
    ret30: 4,
    ret1h: 5,
    v20: 2,
    upperWick: 0.1,
    min15m: 0,
    min4h: 0,
    min24h: 0,
    max24h: 1,
    prevRet5: 0,
    closePosition: 0.8,
    bodyRatio: 0.6,
    outcomes: new Map([['0.01:1', r]]),
  });
  const featureSet = {
    bySymbol: new Map([
      ['AAA', [
        matchingFeature('AAA', 10, 1),
        matchingFeature('AAA', 20, 1),
        matchingFeature('AAA', 110, 1),
      ]],
      ['BBB', [matchingFeature('BBB', 30, 1)]],
    ]),
  };
  const filter = {
    minImp: 2,
    maxImp: 4,
    volN: 20,
    minVol: 1,
    maxVol: 999,
    minRet30: 3,
    maxRet1h: 10,
    maxWick: 0.5,
    minM15: -1,
    minM4h: -1,
    minM24: -1,
    maxM24: 2,
  };
  const segments = [
    ['w1', 0, 100],
    ['w2', 100, 200],
  ];

  const result = evaluateConfig(
    featureSet,
    filter,
    { stopPct: 0.01, targetR: 1, cooldownMinutes: 0, maxTradesPerSymbolPerSegment: 1 },
    segments,
  );

  assert.equal(result.summary.tradeCount, 3);
  assert.equal(result.summary.totalR, 3);
});

test('equalTimeSegments covers the full cache window with a closed final boundary', () => {
  assert.deepEqual(equalTimeSegments(100, 199, 4), [
    ['w1', 100, 125],
    ['w2', 125, 150],
    ['w3', 150, 175],
    ['w4', 175, 200],
  ]);
});

test('recentStableScore rewards recent remove-top stability before raw total', () => {
  const concentrated = {
    summary: { totalR: 30, removeTop5R: 12, maxDrawdownR: 3, winRatePct: 66 },
    segments: [
      { label: 'w4', removeTopR: -2, totalR: 5 },
    ],
  };
  const stableRecent = {
    summary: { totalR: 24, removeTop5R: 8, maxDrawdownR: 4, winRatePct: 62 },
    segments: [
      { label: 'w4', removeTopR: 1, totalR: 3 },
    ],
  };

  assert.ok(recentStableScore(stableRecent) > recentStableScore(concentrated));
});

test('isRecentStableCandidate rejects negative recent-window total', () => {
  const candidate = {
    summary: { tradeCount: 50, winRatePct: 62, totalR: 20, maxDrawdownR: 4 },
    segments: [
      { label: 'w4', tradeCount: 7, totalR: -0.5, removeTopR: 0.2 },
    ],
  };

  assert.equal(isRecentStableCandidate(candidate), false);
});

test('isAllSegmentStableCandidate rejects any negative segment remove-top result', () => {
  const candidate = {
    summary: { tradeCount: 50, winRatePct: 62, totalR: 20, maxDrawdownR: 4 },
    segments: [
      { label: 'w1', tradeCount: 10, totalR: 5, removeTopR: 1 },
      { label: 'w2', tradeCount: 15, totalR: 7, removeTopR: -0.1 },
      { label: 'w3', tradeCount: 12, totalR: 3, removeTopR: 0.5 },
      { label: 'w4', tradeCount: 13, totalR: 5, removeTopR: 0.3 },
    ],
  };

  assert.equal(isAllSegmentStableCandidate(candidate), false);
});

test('robustnessScore prefers a better worst segment over higher raw total', () => {
  const highReturnFragile = {
    summary: { totalR: 30, removeTop5R: 10, maxDrawdownR: 3, winRatePct: 66 },
    minSegmentRemoveTopR: -2,
  };
  const lowerReturnRobust = {
    summary: { totalR: 22, removeTop5R: 8, maxDrawdownR: 4, winRatePct: 62 },
    minSegmentRemoveTopR: -0.2,
  };

  assert.ok(robustnessScore(lowerReturnRobust) > robustnessScore(highReturnFragile));
});

test('parseArgs supports selecting a scan mode', () => {
  assert.equal(parseArgs(['--mode', 'extended']).mode, 'extended');
  assert.equal(parseArgs(['--mode', 'refine']).mode, 'refine');
  assert.equal(parseArgs(['--mode', 'stable45']).mode, 'stable45');
  assert.equal(parseArgs(['--mode', 'symbolcap']).mode, 'symbolcap');
});

test('filterMatches supports pre-entry shape and previous-impulse gates', () => {
  const feature = {
    ret5: 3,
    ret30: 4,
    ret1h: 5,
    v20: 2,
    upperWick: 0.1,
    min15m: 0.2,
    min4h: 0.3,
    min24h: -1,
    max24h: 1,
    prevRet5: 2.2,
    closePosition: 0.8,
    bodyRatio: 0.6,
  };
  const filter = {
    minImp: 2,
    maxImp: 4,
    volN: 20,
    minVol: 1.1,
    maxVol: 999,
    minRet30: 3,
    maxRet1h: 10,
    maxWick: 0.5,
    minM15: 0,
    minM4h: 0,
    minM24: -8,
    maxM24: 8,
    maxPrevRet5: 2,
    minClosePosition: 0.7,
    minBodyRatio: 0.5,
  };

  assert.equal(filterMatches(feature, filter), false);
});

test('buildResearchPresetContract marks the best 5m impulse preset as research-only', () => {
  const candidate = {
    n: 51,
    win: 74.51,
    totalR: 27.44,
    maxDdR: 3.08,
    removeTop5R: 7.933,
    minSegmentRemoveTopR: 0.14,
    filter: {
      minImp: 2.25,
      maxImp: 4.25,
      volN: 20,
      minVol: 1.1,
      maxVol: 999,
      minRet30: 3,
      maxRet1h: 10,
      maxWick: 0.45,
      minM15: -0.2,
      minM4h: 0,
      minM24: -8,
      maxM24: 8,
      maxPrevRet5: 999,
      minClosePosition: 0.8,
      minBodyRatio: 0,
    },
    risk: {
      stopPct: 0.0375,
      targetR: 1.1,
      cooldownMinutes: 60,
    },
  };

  const contract = buildResearchPresetContract(candidate, {
    start: '2026-06-01T00:00:00.000Z',
    end: '2026-07-03T16:15:00.000Z',
  });

  assert.equal(contract.strategyKey, 'market_velocity_5m_direct_impulse');
  assert.equal(contract.status, 'research_only');
  assert.equal(contract.timeframe, '5m');
  assert.equal(contract.entry, 'next_5m_open_after_signal_close');
  assert.equal(contract.promotion.liveEligible, false);
  assert.equal(contract.promotion.requiredNextStep, 'paper_observation');
  assert.equal(contract.filter.minImp, 2.25);
  assert.equal(contract.filter.maxImp, 4.25);
  assert.equal(contract.filter.minClosePosition, 0.8);
  assert.equal(contract.risk.stopPct, 0.0375);
  assert.equal(contract.risk.targetR, 1.1);
  assert.equal(contract.risk.cooldownMinutes, 60);
  assert.equal(contract.evidence.sampleStart, '2026-06-01T00:00:00.000Z');
  assert.equal(contract.evidence.sampleEnd, '2026-07-03T16:15:00.000Z');
  assert.equal(contract.evidence.metrics.n, 51);
  assert.equal(contract.evidence.metrics.win, 74.51);
  assert.equal(contract.evidence.metrics.minSegmentRemoveTopR, 0.14);
});
