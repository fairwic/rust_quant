#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${QUANT_CORE_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_core"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${QUANT_CORE_POSTGRES_DB:="quant_core"}"
: "${TARGET_EXCHANGE:="binance"}"
: "${TARGET_STRATEGY_KEY:="vegas"}"
: "${TOP_N:="5"}"
: "${RECENT_WINDOW:="32"}"
: "${MIN_CONFIRMED_FLOOR:="120"}"

run_quant_sql() {
    if command -v psql >/dev/null 2>&1; then
        psql "${QUANT_CORE_DATABASE_URL}" -v ON_ERROR_STOP=1 "$@"
        return
    fi

    if command -v podman >/dev/null 2>&1 &&
        podman container exists "${POSTGRES_CONTAINER}" >/dev/null 2>&1; then
        podman exec -i "${POSTGRES_CONTAINER}" psql \
            -U "${POSTGRES_USER}" \
            -d "${QUANT_CORE_POSTGRES_DB}" \
            -v ON_ERROR_STOP=1 \
            "$@"
        return
    fi

    echo "Refusing to run: neither psql nor podman container '${POSTGRES_CONTAINER}' is available for quant_core." >&2
    exit 2
}

query_quant_scalar() {
    run_quant_sql -Atc "$1"
}

table_suffix_for_timeframe() {
    local timeframe="$1"
    case "${timeframe}" in
        1M) printf '1M' ;;
        *) printf '%s' "${timeframe}" | tr '[:upper:]' '[:lower:]' ;;
    esac
}

timeframe_to_seconds() {
    local timeframe="$1"
    case "${timeframe}" in
        1m) echo 60 ;;
        3m) echo 180 ;;
        5m) echo 300 ;;
        15m) echo 900 ;;
        30m) echo 1800 ;;
        1H) echo 3600 ;;
        2H) echo 7200 ;;
        4H) echo 14400 ;;
        6H) echo 21600 ;;
        12H) echo 43200 ;;
        1D|1Dutc|1d|1dutc) echo 86400 ;;
        *)
            echo "Unsupported timeframe: ${timeframe}" >&2
            exit 2
            ;;
    esac
}

timeframe_rank_score() {
    local timeframe="$1"
    case "${timeframe}" in
        1m) echo 70 ;;
        3m) echo 64 ;;
        5m) echo 58 ;;
        15m) echo 50 ;;
        30m) echo 38 ;;
        1H) echo 28 ;;
        2H) echo 18 ;;
        4H) echo 8 ;;
        6H) echo 4 ;;
        12H) echo 2 ;;
        1D|1Dutc|1d|1dutc) echo 1 ;;
        *) echo 0 ;;
    esac
}

clamp_float() {
    local value="$1"
    local min_value="$2"
    local max_value="$3"
    awk -v v="${value}" -v lo="${min_value}" -v hi="${max_value}" '
        BEGIN {
            if (v < lo) v = lo;
            if (v > hi) v = hi;
            printf "%.4f", v;
        }
    '
}

compute_candidate_score() {
    local timeframe="$1"
    local confirmed_count="$2"
    local min_k_line_num="$3"
    local age_secs="$4"
    local timeframe_secs="$5"
    local recent_range_pct="$6"
    local recent_move_pct="$7"
    local volume_spike_ratio="$8"
    local secs_to_next_confirm="$9"

    local cadence_score coverage_score freshness_score activity_score wait_score

    cadence_score="$(timeframe_rank_score "${timeframe}")"
    coverage_score="$(
        awk -v count="${confirmed_count}" -v need="${min_k_line_num}" '
            BEGIN {
                if (need <= 0) {
                    printf "%.4f", 15;
                    exit;
                }
                ratio = count / need;
                if (ratio > 1.5) ratio = 1.5;
                printf "%.4f", ratio * 10;
            }
        '
    )"
    freshness_score="$(
        awk -v age="${age_secs}" -v frame="${timeframe_secs}" '
            BEGIN {
                if (frame <= 0) {
                    printf "%.4f", 0;
                    exit;
                }
                ratio = age / frame;
                if (ratio <= 1.2) printf "%.4f", 20;
                else if (ratio <= 2.5) printf "%.4f", 14;
                else if (ratio <= 4.0) printf "%.4f", 8;
                else printf "%.4f", 2;
            }
        '
    )"
    activity_score="$(
        awk -v range="${recent_range_pct}" -v move="${recent_move_pct}" -v spike="${volume_spike_ratio}" '
            BEGIN {
                range_score = range * 600;
                if (range_score > 18) range_score = 18;
                move_score = move * 500;
                if (move_score > 12) move_score = 12;
                spike_score = (spike - 1.0) * 8;
                if (spike_score < 0) spike_score = 0;
                if (spike_score > 8) spike_score = 8;
                printf "%.4f", range_score + move_score + spike_score;
            }
        '
    )"
    wait_score="$(
        awk -v wait="${secs_to_next_confirm}" -v frame="${timeframe_secs}" '
            BEGIN {
                if (frame <= 0) {
                    printf "%.4f", 0;
                    exit;
                }
                ratio = wait / frame;
                if (ratio <= 0.10) printf "%.4f", 8;
                else if (ratio <= 0.25) printf "%.4f", 6;
                else if (ratio <= 0.50) printf "%.4f", 3;
                else printf "%.4f", 0;
            }
        '
    )"

    awk \
        -v cadence="${cadence_score}" \
        -v coverage="${coverage_score}" \
        -v freshness="${freshness_score}" \
        -v activity="${activity_score}" \
        -v wait="${wait_score}" '
        BEGIN {
            printf "%.4f", cadence + coverage + freshness + activity + wait;
        }
    '
}

recommended_timeout_secs() {
    local timeframe_secs="$1"
    awk -v frame="${timeframe_secs}" '
        BEGIN {
            timeout = frame * 6;
            if (timeout < 1800) timeout = 1800;
            if (timeout > 28800) timeout = 28800;
            printf "%d", timeout;
        }
    '
}

format_ts_shanghai() {
    local ts_ms="$1"
    run_quant_sql -Atc "SELECT to_char(to_timestamp(${ts_ms} / 1000.0) AT TIME ZONE 'Asia/Shanghai', 'YYYY-MM-DD HH24:MI:SS');"
}

echo "Scanning quant_core strategy_configs and Binance candle split tables"
echo "  strategy_key: ${TARGET_STRATEGY_KEY}"
echo "  exchange: ${TARGET_EXCHANGE}"
echo "  recent_window: ${RECENT_WINDOW}"
echo "  top_n: ${TOP_N}"

candidate_rows="$(run_quant_sql -F $'\t' -Atc "
WITH deduped AS (
    SELECT
        sc.strategy_key,
        sc.symbol,
        sc.timeframe,
        sc.version,
        COALESCE((sc.config->>'min_k_line_num')::int, 60) AS min_k_line_num,
        COALESCE(NULLIF(sc.config->>'period', ''), sc.timeframe) AS config_period,
        regexp_replace(COALESCE(sc.config->'_migration'->>'tags', ''), E'[\\n\\r\\t]+', ' ', 'g') AS tags,
        COALESCE(sc.updated_at, sc.created_at) AS sort_at
    FROM strategy_configs sc
    WHERE sc.exchange = '${TARGET_EXCHANGE}'
      AND sc.enabled = true
      AND sc.strategy_key = '${TARGET_STRATEGY_KEY}'
      AND sc.version NOT LIKE 'smoke-binance-websocket-natural-%'
)
SELECT strategy_key, symbol, timeframe, version, min_k_line_num, config_period, tags
FROM deduped
ORDER BY sort_at DESC NULLS LAST, symbol, timeframe, version;
")"

if [[ -z "${candidate_rows}" ]]; then
    echo "No enabled ${TARGET_EXCHANGE} ${TARGET_STRATEGY_KEY} configs found in quant_core.strategy_configs." >&2
    exit 1
fi

TMP_CANDIDATES="$(mktemp "${TMPDIR:-/tmp}/binance_natural_probe_candidates.XXXXXX")"
TMP_INPUT="$(mktemp "${TMPDIR:-/tmp}/binance_natural_probe_input.XXXXXX")"
trap 'rm -f "${TMP_CANDIDATES}" "${TMP_INPUT}"' EXIT
printf '%s\n' "${candidate_rows}" >"${TMP_INPUT}"

NOW_SECS="$(date +%s)"

while IFS=$'\t' read -r strategy_key symbol timeframe version min_k_line_num config_period tags <&3; do
    [[ -n "${strategy_key}" ]] || continue

    table_suffix="$(table_suffix_for_timeframe "${timeframe}")"
    table_name="$(printf '%s' "${symbol}" | tr '[:upper:]' '[:lower:]')_candles_${table_suffix}"
    table_exists="$(query_quant_scalar "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE schemaname = 'public' AND tablename = '${table_name}');")"
    if [[ "${table_exists}" != "t" ]]; then
        continue
    fi

    metrics="$(
        run_quant_sql -F $'\t' -Atc "
WITH confirmed AS (
    SELECT
        ts,
        o::double precision AS o,
        h::double precision AS h,
        l::double precision AS l,
        c::double precision AS c,
        vol::double precision AS vol
    FROM \"${table_name}\"
    WHERE confirm = '1'
),
recent AS (
    SELECT *
    FROM confirmed
    ORDER BY ts DESC
    LIMIT ${RECENT_WINDOW}
),
recent_asc AS (
    SELECT *
    FROM recent
    ORDER BY ts ASC
)
SELECT
    COALESCE((SELECT COUNT(*) FROM confirmed), 0),
    COALESCE((SELECT MAX(ts) FROM confirmed), 0),
    COALESCE((SELECT COUNT(*) FROM recent), 0),
    COALESCE((SELECT AVG(ABS(c - o) / NULLIF(ABS(o), 0)) FROM recent), 0),
    COALESCE((SELECT (MAX(h) - MIN(l)) / NULLIF(AVG(c), 0) FROM recent), 0),
    COALESCE((
        SELECT ABS(last_row.c - first_row.o) / NULLIF(ABS(first_row.o), 0)
        FROM (SELECT o FROM recent_asc ORDER BY ts ASC LIMIT 1) first_row
        CROSS JOIN (SELECT c FROM recent_asc ORDER BY ts DESC LIMIT 1) last_row
    ), 0),
    COALESCE((SELECT MAX(vol) / NULLIF(AVG(vol), 0) FROM recent), 0)
;"
    )"

    IFS=$'\t' read -r confirmed_count last_ts recent_count avg_body_pct recent_range_pct recent_move_pct volume_spike_ratio <<<"${metrics}"
    confirmed_count="${confirmed_count:-0}"
    last_ts="${last_ts:-0}"
    recent_count="${recent_count:-0}"
    avg_body_pct="${avg_body_pct:-0}"
    recent_range_pct="${recent_range_pct:-0}"
    recent_move_pct="${recent_move_pct:-0}"
    volume_spike_ratio="${volume_spike_ratio:-0}"

    if (( confirmed_count < MIN_CONFIRMED_FLOOR || recent_count == 0 || last_ts == 0 )); then
        continue
    fi

    timeframe_secs="$(timeframe_to_seconds "${timeframe}")"
    last_secs=$(( last_ts / 1000 ))
    age_secs=$(( NOW_SECS - last_secs ))
    mod_secs=$(( NOW_SECS % timeframe_secs ))
    secs_to_next_confirm=$(( timeframe_secs - mod_secs ))
    if (( secs_to_next_confirm == timeframe_secs )); then
        secs_to_next_confirm=0
    fi

    score="$(compute_candidate_score \
        "${timeframe}" \
        "${confirmed_count}" \
        "${min_k_line_num}" \
        "${age_secs}" \
        "${timeframe_secs}" \
        "${recent_range_pct}" \
        "${recent_move_pct}" \
        "${volume_spike_ratio}" \
        "${secs_to_next_confirm}")"
    live_timeout_secs="$(recommended_timeout_secs "${timeframe_secs}")"
    last_confirmed_at="$(format_ts_shanghai "${last_ts}")"

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "${score}" \
        "${symbol}" \
        "${timeframe}" \
        "${version}" \
        "${min_k_line_num}" \
        "${confirmed_count}" \
        "${last_confirmed_at}" \
        "${age_secs}" \
        "${secs_to_next_confirm}" \
        "${recent_count}" \
        "${avg_body_pct}" \
        "${recent_range_pct}" \
        "${recent_move_pct}" \
        "${volume_spike_ratio}" \
        "${live_timeout_secs}" \
        "${strategy_key}" \
        "${tags}" >>"${TMP_CANDIDATES}"
done 3<"${TMP_INPUT}"

if [[ ! -s "${TMP_CANDIDATES}" ]]; then
    echo "No candidates remained after checking confirmed candles and Binance split tables." >&2
    exit 1
fi

echo
echo "recommended_candidates"
echo "rank | score | symbol | timeframe | version | confirmed | min_k_line_num | age_min | next_confirm_sec | recent_range_pct | recent_move_pct | volume_spike_ratio | tags"

rank=0
while IFS=$'\t' read -r score symbol timeframe version min_k_line_num confirmed_count last_confirmed_at age_secs secs_to_next_confirm recent_count avg_body_pct recent_range_pct recent_move_pct volume_spike_ratio live_timeout_secs strategy_key tags; do
    rank=$((rank + 1))
    age_min="$(
        awk -v age="${age_secs}" 'BEGIN { printf "%.1f", age / 60.0 }'
    )"
    printf '%d | %.2f | %s | %s | %s | %s | %s | %s | %s | %.4f | %.4f | %.4f | %s\n' \
        "${rank}" \
        "${score}" \
        "${symbol}" \
        "${timeframe}" \
        "${version}" \
        "${confirmed_count}" \
        "${min_k_line_num}" \
        "${age_min}" \
        "${secs_to_next_confirm}" \
        "${recent_range_pct}" \
        "${recent_move_pct}" \
        "${volume_spike_ratio}" \
        "${tags}"

    echo "  last_confirmed_at_shanghai=${last_confirmed_at}"
    echo "  Suggested natural probe command:"
    runtime_version="$(
        printf 'smoke-binance-websocket-natural-%s-%s' \
            "$(printf '%s' "${symbol}" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//')" \
            "$(printf '%s' "${timeframe}" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//')"
    )"
    echo "    SMOKE_SYMBOL='${symbol}' SMOKE_PERIOD='${timeframe}' SMOKE_STRATEGY_KEY='${strategy_key}' SMOKE_SOURCE_STRATEGY_VERSION='${version}' SMOKE_STRATEGY_VERSION='${runtime_version}' SMOKE_MIN_K_LINE_NUM='${min_k_line_num}' SMOKE_LIVE_TIMEOUT_SECS='${live_timeout_secs}' ${REPO_ROOT}/scripts/dev/run_binance_websocket_natural_probe.sh"
    echo

    if (( rank >= TOP_N )); then
        break
    fi
done < <(sort -t $'\t' -k1,1nr "${TMP_CANDIDATES}")

echo "Heuristic notes:"
echo "  - score favors shorter timeframes, fresher candles, larger recent confirmed-candle range, and visible volume spikes."
echo "  - candidates come from quant_core.strategy_configs plus matching public.*_candles_* tables."
echo "  - this script does not set RUST_QUANT_SMOKE_FORCE_SIGNAL."
