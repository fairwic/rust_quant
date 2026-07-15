#!/usr/bin/env node

import { execFile } from 'node:child_process';
import fs from 'node:fs';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';

const execFileAsync = promisify(execFile);

const DEFAULT_OKX_BASE = 'https://www.okx.com';
const DEFAULT_RANK_CACHE = '/tmp/mv5m_rankhot30_20260601_20260703_1615.json';
const DEFAULT_MACRO_CACHE = '/tmp/mv5m_macro_btc_eth_20260601_20260703_1615.json';
const DEFAULT_OUTPUT_RANK_CACHE = '/tmp/mv5m_rankhot30_extended.json';
const DEFAULT_OUTPUT_MACRO_CACHE = '/tmp/mv5m_macro_btc_eth_extended.json';
const FIVE_MINUTE_MS = 5 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, 'utf8'));
}

function writeJson(path, value) {
  fs.writeFileSync(path, `${JSON.stringify(value)}\n`);
}

export async function mapWithConcurrency(items, concurrency, mapper) {
  const results = new Array(items.length);
  let nextIndex = 0;
  async function worker() {
    while (nextIndex < items.length) {
      const index = nextIndex;
      nextIndex += 1;
      results[index] = await mapper(items[index], index);
    }
  }
  const workerCount = Math.min(Math.max(1, concurrency), items.length);
  await Promise.all(Array.from({ length: workerCount }, worker));
  return results;
}

function parsePositiveInteger(value, flag) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${flag} must be a positive integer`);
  }
  return parsed;
}

function parseNonNegativeInteger(value, flag) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error(`${flag} must be a non-negative integer`);
  }
  return parsed;
}

function nextArg(args, flag) {
  const value = args.shift();
  if (!value) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

export function symbolsFromCache(cache) {
  return Object.keys(cache.candlesBySymbol ?? {});
}

export function buildOkxHistoryCandlesUrl({
  baseUrl = DEFAULT_OKX_BASE,
  symbol,
  bar = '5m',
  afterMs,
  limit = 100,
}) {
  const url = new URL('/api/v5/market/history-candles', baseUrl);
  url.searchParams.set('instId', symbol);
  url.searchParams.set('bar', bar);
  url.searchParams.set('limit', String(limit));
  if (afterMs !== undefined) {
    url.searchParams.set('after', String(afterMs));
  }
  return url.toString();
}

function parseOkxCandleRow(row) {
  return {
    ts: Number(row[0]),
    open: Number(row[1]),
    high: Number(row[2]),
    low: Number(row[3]),
    close: Number(row[4]),
    vol: Number(row[5]),
  };
}

export function parseOkxCandleRows(rows) {
  return rows.map(parseOkxCandleRow).sort((left, right) => left.ts - right.ts);
}

export function mergeCandleRows(existingRows, fetchedRows, startMs, endMs) {
  const rowsByTs = new Map();
  for (const row of [...existingRows, ...fetchedRows]) {
    if (startMs <= row.ts && row.ts <= endMs) {
      rowsByTs.set(row.ts, row);
    }
  }
  return [...rowsByTs.values()].sort((left, right) => left.ts - right.ts);
}

export function missingFetchRanges(existingRows, startMs, endMs, candleMs = FIVE_MINUTE_MS) {
  const rows = mergeCandleRows(existingRows, [], startMs, endMs);
  if (rows.length === 0) {
    return [[startMs, endMs]];
  }
  const ranges = [];
  if (rows[0].ts > startMs) {
    ranges.push([startMs, rows[0].ts - candleMs]);
  }
  for (let index = 1; index < rows.length; index += 1) {
    const expected = rows[index - 1].ts + candleMs;
    if (rows[index].ts > expected) {
      ranges.push([expected, rows[index].ts - candleMs]);
    }
  }
  const afterLast = rows.at(-1).ts + candleMs;
  if (afterLast <= endMs) {
    ranges.push([afterLast, endMs]);
  }
  return ranges.filter(([start, end]) => start <= end);
}

async function curlJson(url) {
  const { stdout } = await execFileAsync(
    'curl',
    ['-fsS', '-H', 'User-Agent: rust-quant-market-velocity-5m-research/1.0', url],
    { maxBuffer: 20 * 1024 * 1024 },
  );
  return JSON.parse(stdout);
}

export async function collectOkxHistoryCandles({
  symbol,
  startMs,
  endMs,
  initialAfterMs,
  baseUrl = DEFAULT_OKX_BASE,
  bar = '5m',
  limit = 100,
  requestSleepMs = 0,
  fetchPage = curlJson,
}) {
  const candlesByTs = new Map();
  let afterMs = initialAfterMs;
  const maxPages = Math.ceil((endMs - startMs + FIVE_MINUTE_MS) / (FIVE_MINUTE_MS * limit)) + 5;
  for (let pageIndex = 0; pageIndex < maxPages; pageIndex += 1) {
    const url = buildOkxHistoryCandlesUrl({ baseUrl, symbol, bar, afterMs, limit });
    const payload = await fetchPage(url, symbol);
    if (payload.code !== '0') {
      throw new Error(`OKX history-candles returned code=${payload.code} msg=${payload.msg ?? ''} symbol=${symbol}`);
    }
    if (!payload.data?.length) {
      break;
    }
    let pageOldest = Number.POSITIVE_INFINITY;
    for (const candle of parseOkxCandleRows(payload.data)) {
      pageOldest = Math.min(pageOldest, candle.ts);
      if (startMs <= candle.ts && candle.ts <= endMs) {
        candlesByTs.set(candle.ts, candle);
      }
    }
    if (pageOldest <= startMs) {
      break;
    }
    if (afterMs !== undefined && pageOldest >= afterMs) {
      break;
    }
    afterMs = pageOldest;
    if (requestSleepMs > 0) {
      await sleep(requestSleepMs);
    }
  }
  return [...candlesByTs.values()].sort((left, right) => left.ts - right.ts);
}

function parseArgs(argv) {
  const args = {
    sourceRankCache: DEFAULT_RANK_CACHE,
    sourceMacroCache: DEFAULT_MACRO_CACHE,
    outputRankCache: DEFAULT_OUTPUT_RANK_CACHE,
    outputMacroCache: DEFAULT_OUTPUT_MACRO_CACHE,
    reuseRankCache: undefined,
    reuseMacroCache: undefined,
    okxBase: DEFAULT_OKX_BASE,
    days: 45,
    limit: 100,
    concurrency: 4,
    requestSleepMs: 0,
  };
  const rest = [...argv];
  while (rest.length > 0) {
    const flag = rest.shift();
    if (flag === '--source-rank-cache') {
      args.sourceRankCache = nextArg(rest, flag);
    } else if (flag === '--source-macro-cache') {
      args.sourceMacroCache = nextArg(rest, flag);
    } else if (flag === '--output-rank-cache') {
      args.outputRankCache = nextArg(rest, flag);
    } else if (flag === '--output-macro-cache') {
      args.outputMacroCache = nextArg(rest, flag);
    } else if (flag === '--reuse-rank-cache') {
      args.reuseRankCache = nextArg(rest, flag);
    } else if (flag === '--reuse-macro-cache') {
      args.reuseMacroCache = nextArg(rest, flag);
    } else if (flag === '--okx-base') {
      args.okxBase = nextArg(rest, flag);
    } else if (flag === '--days') {
      args.days = parsePositiveInteger(nextArg(rest, flag), flag);
    } else if (flag === '--end-ms') {
      args.endMs = parsePositiveInteger(nextArg(rest, flag), flag);
    } else if (flag === '--limit') {
      args.limit = parsePositiveInteger(nextArg(rest, flag), flag);
    } else if (flag === '--concurrency') {
      args.concurrency = parsePositiveInteger(nextArg(rest, flag), flag);
    } else if (flag === '--request-sleep-ms') {
      args.requestSleepMs = parseNonNegativeInteger(nextArg(rest, flag), flag);
    } else if (flag === '--help') {
      args.help = true;
    } else {
      throw new Error(`unknown argument: ${flag}`);
    }
  }
  return args;
}

function printUsage() {
  console.log(`Usage: node scripts/research/market_velocity_5m_cache_builder.mjs [--days 45] [--end-ms MS] [--source-rank-cache PATH] [--source-macro-cache PATH] [--reuse-rank-cache PATH] [--reuse-macro-cache PATH] [--output-rank-cache PATH] [--output-macro-cache PATH] [--concurrency 4] [--request-sleep-ms 0]

Builds read-only OKX 5m candle caches for Market Velocity direct-impulse research.
The source caches provide the symbol universe only; no database or trading API is used.`);
}

async function fetchCacheForSymbols(symbols, options) {
  const candlesBySymbol = {};
  await mapWithConcurrency(symbols, options.concurrency, async (symbol, index) => {
    const existingRows = options.reuseCache?.candlesBySymbol?.[symbol] ?? [];
    const ranges = missingFetchRanges(existingRows, options.startMs, options.endMs);
    process.stderr.write(`fetch ${index + 1}/${symbols.length} ${symbol} missing_ranges=${ranges.length}\n`);
    const fetchedRows = [];
    for (const [startMs, endMs] of ranges) {
      fetchedRows.push(
        ...(await collectOkxHistoryCandles({
          symbol,
          startMs,
          endMs,
          initialAfterMs: endMs + FIVE_MINUTE_MS,
          baseUrl: options.okxBase,
          limit: options.limit,
          requestSleepMs: options.requestSleepMs,
        })),
      );
    }
    candlesBySymbol[symbol] = mergeCandleRows(
      existingRows,
      fetchedRows,
      options.startMs,
      options.endMs,
    );
  });
  return candlesBySymbol;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printUsage();
    return;
  }
  const sourceRank = readJson(args.sourceRankCache);
  const sourceMacro = readJson(args.sourceMacroCache);
  const reuseRank = args.reuseRankCache ? readJson(args.reuseRankCache) : undefined;
  const reuseMacro = args.reuseMacroCache ? readJson(args.reuseMacroCache) : undefined;
  const endMs = args.endMs ?? sourceRank.end;
  const startMs = endMs - args.days * DAY_MS;
  const options = {
    startMs,
    endMs,
    okxBase: args.okxBase,
    limit: args.limit,
    concurrency: args.concurrency,
    requestSleepMs: args.requestSleepMs,
  };
  const rankSymbols = symbolsFromCache(sourceRank);
  const macroSymbols = symbolsFromCache(sourceMacro);
  const rankCandlesBySymbol = await fetchCacheForSymbols(rankSymbols, {
    ...options,
    reuseCache: reuseRank,
  });
  const macroCandlesBySymbol = await fetchCacheForSymbols(macroSymbols, {
    ...options,
    reuseCache: reuseMacro,
  });
  writeJson(args.outputRankCache, {
    start: startMs,
    end: endMs,
    sourceUniverseCache: args.sourceRankCache,
    reusedCache: args.reuseRankCache,
    symbolRows: sourceRank.symbolRows ?? [],
    candlesBySymbol: rankCandlesBySymbol,
  });
  writeJson(args.outputMacroCache, {
    start: startMs,
    end: endMs,
    sourceUniverseCache: args.sourceMacroCache,
    reusedCache: args.reuseMacroCache,
    candlesBySymbol: macroCandlesBySymbol,
  });
  process.stderr.write(`wrote ${args.outputRankCache}\nwrote ${args.outputMacroCache}\n`);
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
