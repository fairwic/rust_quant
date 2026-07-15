import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  buildOkxHistoryCandlesUrl,
  collectOkxHistoryCandles,
  mapWithConcurrency,
  mergeCandleRows,
  missingFetchRanges,
  parseOkxCandleRows,
  symbolsFromCache,
} from './market_velocity_5m_cache_builder.mjs';

test('buildOkxHistoryCandlesUrl keeps OKX pagination parameters explicit', () => {
  const url = buildOkxHistoryCandlesUrl({
    baseUrl: 'https://www.okx.com/',
    symbol: 'BTC-USDT-SWAP',
    bar: '5m',
    afterMs: 1783098000000,
    limit: 100,
  });

  assert.equal(
    url,
    'https://www.okx.com/api/v5/market/history-candles?instId=BTC-USDT-SWAP&bar=5m&limit=100&after=1783098000000',
  );
});

test('parseOkxCandleRows maps OKX rows to scan cache candles in ascending time', () => {
  const rows = parseOkxCandleRows([
    ['2000', '10', '12', '9', '11', '100', '1', '1000', '1'],
    ['1000', '8', '9', '7', '8.5', '50', '0.5', '400', '1'],
  ]);

  assert.deepEqual(rows, [
    { ts: 1000, open: 8, high: 9, low: 7, close: 8.5, vol: 50 },
    { ts: 2000, open: 10, high: 12, low: 9, close: 11, vol: 100 },
  ]);
});

test('collectOkxHistoryCandles paginates older candles, filters range, and dedupes', async () => {
  const requestedUrls = [];
  const pages = [
    {
      code: '0',
      data: [
        ['3000', '3', '4', '2', '3.5', '30'],
        ['2000', '2', '3', '1', '2.5', '20'],
      ],
    },
    {
      code: '0',
      data: [
        ['2000', '2', '3', '1', '2.5', '20'],
        ['1000', '1', '2', '0.5', '1.5', '10'],
      ],
    },
  ];

  const candles = await collectOkxHistoryCandles({
    symbol: 'BTC-USDT-SWAP',
    startMs: 1500,
    endMs: 3000,
    limit: 2,
    fetchPage: async (url) => {
      requestedUrls.push(url);
      return pages.shift();
    },
  });

  assert.equal(requestedUrls.length, 2);
  assert.ok(requestedUrls[1].endsWith('&after=2000'));
  assert.deepEqual(candles.map((row) => row.ts), [2000, 3000]);
});

test('collectOkxHistoryCandles can start pagination from an older right boundary', async () => {
  const requestedUrls = [];
  await collectOkxHistoryCandles({
    symbol: 'BTC-USDT-SWAP',
    startMs: 1000,
    endMs: 2000,
    initialAfterMs: 2500,
    limit: 2,
    fetchPage: async (url) => {
      requestedUrls.push(url);
      return {
        code: '0',
        data: [['2000', '2', '3', '1', '2.5', '20']],
      };
    },
  });

  assert.ok(requestedUrls[0].endsWith('&after=2500'));
});

test('symbolsFromCache preserves the source cache universe order', () => {
  assert.deepEqual(
    symbolsFromCache({
      candlesBySymbol: {
        'OPG-USDT-SWAP': [],
        'BIO-USDT-SWAP': [],
      },
    }),
    ['OPG-USDT-SWAP', 'BIO-USDT-SWAP'],
  );
});

test('mapWithConcurrency preserves order while bounding active work', async () => {
  let active = 0;
  let maxActive = 0;
  const results = await mapWithConcurrency([3, 1, 2, 4], 2, async (value) => {
    active += 1;
    maxActive = Math.max(maxActive, active);
    await new Promise((resolve) => setTimeout(resolve, value));
    active -= 1;
    return value * 10;
  });

  assert.deepEqual(results, [30, 10, 20, 40]);
  assert.equal(maxActive, 2);
});

test('mergeCandleRows dedupes by timestamp, filters range, and sorts ascending', () => {
  const merged = mergeCandleRows(
    [
      { ts: 1000, close: 1 },
      { ts: 2000, close: 2 },
      { ts: 5000, close: 5 },
    ],
    [
      { ts: 2000, close: 22 },
      { ts: 3000, close: 3 },
    ],
    1500,
    3500,
  );

  assert.deepEqual(merged, [
    { ts: 2000, close: 22 },
    { ts: 3000, close: 3 },
  ]);
});

test('missingFetchRanges finds only gaps not covered by a reusable cache', () => {
  const ranges = missingFetchRanges(
    [
      { ts: 2000 },
      { ts: 3000 },
      { ts: 5000 },
    ],
    1000,
    6000,
    1000,
  );

  assert.deepEqual(ranges, [
    [1000, 1000],
    [4000, 4000],
    [6000, 6000],
  ]);
});
