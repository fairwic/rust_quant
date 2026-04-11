SELECT
    date_trunc('hour', t.block_time) AS hour_bucket,
    COUNT(*) AS whale_transfer_count,
    SUM(t.amount_usd) AS whale_transfer_usd,
    SUM(
        CASE
            WHEN lower(t.to) IN (
                SELECT lower(address)
                FROM labels.addresses
                WHERE blockchain = 'ethereum'
                  AND category IN ('cex', 'bridge')
            )
            OR lower(t.from) IN (
                SELECT lower(address)
                FROM labels.addresses
                WHERE blockchain = 'ethereum'
                  AND category IN ('cex', 'bridge')
            )
            THEN t.amount_usd
            ELSE 0
        END
    ) AS exchange_tagged_transfer_usd
FROM tokens.transfers t
WHERE t.blockchain = 'ethereum'
  AND t.symbol = '{{symbol}}'
  AND t.block_time >= CAST('{{start_time}}' AS TIMESTAMP)
  AND t.block_time < CAST('{{end_time}}' AS TIMESTAMP)
  AND t.amount_usd >= CAST('{{min_usd}}' AS DOUBLE)
GROUP BY 1
ORDER BY 1;
