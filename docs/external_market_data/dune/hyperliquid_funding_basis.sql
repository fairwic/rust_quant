WITH perp_points AS (
    SELECT
        date_trunc('hour', time) AS hour_bucket,
        coin AS asset,
        AVG(funding) AS funding_rate,
        AVG(premium) AS premium,
        AVG(mark_px) AS mark_px,
        AVG(oracle_px) AS oracle_px,
        AVG(open_interest) AS open_interest
    FROM hyperliquid.market_data
    WHERE coin = '{{symbol}}'
      AND time >= from_iso8601_timestamp('{{start_time}}')
      AND time < from_iso8601_timestamp('{{end_time}}')
    GROUP BY 1, 2
)
SELECT
    hour_bucket,
    asset,
    funding_rate,
    premium,
    ((mark_px - oracle_px) / NULLIF(oracle_px, 0)) * 10000 AS basis_bps,
    open_interest
FROM perp_points
ORDER BY 1;
