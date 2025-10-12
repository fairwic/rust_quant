WITH trade_stats AS (
    SELECT
        back_test_id,
        bars_after,
        COUNT(DISTINCT concat(inst_id, open_time)) as total_trades,
        SUM(is_profitable) as profitable_trades,
        AVG(price_change_percent) as avg_price_change
    FROM back_test_analysis
    GROUP BY back_test_id, bars_after
)
SELECT
    back_test_id,
    bars_after as 'K线数',
    total_trades as '总交易数',
    profitable_trades as '盈利次数',
    ROUND(profitable_trades * 100.0 / total_trades, 2) as '胜率%',
    ROUND(avg_price_change, 2) as '平均收益%'
FROM trade_stats
ORDER BY back_test_id, bars_after;



WITH ranked_results AS (
    SELECT
        *,
        ROW_NUMBER() OVER (PARTITION BY inst_type ORDER BY CAST(final_fund AS DECIMAL(20, 2)) DESC) as `rank`
    FROM
        `back_test_log`
    WHERE
        strategy_type = "UtBootShort"
        AND win_rate > 0.8
        AND open_positions_num > 20
)
SELECT
    *
FROM
    ranked_results
WHERE
    `rank` = 1
ORDER BY
    CAST(final_fund AS DECIMAL(20, 2)) DESC,
    open_positions_num DESC;

    SELECT
    	*
    FROM
    	back_test_log
    WHERE
    	1 = 1
    	AND open_positions_num > 10
    -- 			AND TIME = "1D"
    	AND win_rate > 0.8
    	AND strategy_type = "UtBoot"
    ORDER BY
    	CAST(
    	final_fund AS DECIMAL ( 20, 0 )) DESC;


 SHOW VARIABLES LIKE 'max_connections';
 SHOW STATUS LIKE 'Threads_connected';

