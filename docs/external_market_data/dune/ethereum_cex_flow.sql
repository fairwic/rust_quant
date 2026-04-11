WITH labeled_flows AS (
    SELECT
        date_trunc('hour', t.block_time) AS hour_bucket,
        CASE
            WHEN lower(t.to) IN (
                SELECT lower(address)
                FROM labels.addresses
                WHERE blockchain = 'ethereum'
                  AND category = 'cex'
            ) THEN 'inflow'
            WHEN lower(t.from) IN (
                SELECT lower(address)
                FROM labels.addresses
                WHERE blockchain = 'ethereum'
                  AND category = 'cex'
            ) THEN 'outflow'
            ELSE 'other'
        END AS flow_side,
        t.amount_usd
    FROM tokens.transfers t
    WHERE t.blockchain = 'ethereum'
      AND t.symbol = '{{symbol}}'
      AND t.block_time >= CAST('{{start_time}}' AS TIMESTAMP)
      AND t.block_time < CAST('{{end_time}}' AS TIMESTAMP)
      AND t.amount_usd >= CAST('{{min_usd}}' AS DOUBLE)
)
SELECT
    hour_bucket,
    SUM(CASE WHEN flow_side = 'inflow' THEN amount_usd ELSE 0 END) AS cex_inflow_usd,
    SUM(CASE WHEN flow_side = 'outflow' THEN amount_usd ELSE 0 END) AS cex_outflow_usd,
    SUM(CASE
        WHEN flow_side = 'inflow' THEN amount_usd
        WHEN flow_side = 'outflow' THEN -amount_usd
        ELSE 0
    END) AS netflow_usd
FROM labeled_flows
GROUP BY 1
ORDER BY 1;
