
CREATE TABLE `tickers_data` (
  `id` int NOT NULL AUTO_INCREMENT,
  `inst_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '产品类型',
  `inst_id` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '产品ID',
  `last` varchar(255) NOT NULL COMMENT '最新成交价',
  `last_sz` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '最新成交的数量',
  `ask_px` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '卖一价',
  `ask_sz` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '卖一价对应的数量',
  `bid_px` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '买一价',
  `bid_sz` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '买一价对应的数量',
  `open24h` varchar(255) NOT NULL COMMENT '24小时开盘价',
  `high24h` varchar(255) NOT NULL COMMENT '24小时最高价',
  `low24h` varchar(255) NOT NULL COMMENT '24小时最低价',
  `vol_ccy24h` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '24小时成交量，以币为单位。如果是衍生品合约，数值为交易货币的数量。如果是币币/币币杠杆，数值为计价货币的数量',
  `vol24h` varchar(255) NOT NULL COMMENT '24小时成交量，以张为单位。如果是衍生品合约，数值为合约的张数。如果是币币/币币杠杆，数值为交易货币的数量',
  `sod_utc0` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT 'UTC+0 时开盘价',
  `sod_utc8` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT 'UTC+8 时开盘价',
  `ts` bigint NOT NULL COMMENT 'ticker数据产生时间，Unix时间戳的毫秒数格式，如 1597026383085',
  PRIMARY KEY (`id`) USING BTREE,
  UNIQUE KEY `inst_type_2` (`inst_type`,`inst_id`),
  KEY `inst_type` (`inst_type`)
) ENGINE=InnoDB AUTO_INCREMENT=6808 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci COMMENT='Ticker 数据表';


CREATE TABLE `strategy_job_signal_log` (
  `id` int NOT NULL AUTO_INCREMENT,
  `inst_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '交易产品id',
  `time` varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '交易周期',
  `strategy_type` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '策略类型',
  `strategy_result` varchar(500) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '策略结果',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=307 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci COMMENT='策略任务信号记录表';


CREATE TABLE `swap_orders` (
  `id` int NOT NULL AUTO_INCREMENT,
  `uuid` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '策略周期唯一值（时间-周期-策略类型-产品id-side-postside）\r\n示例2024+0625+4h+ut_boot+btc-usdt-swap+buy+short',
  `okx_ord_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT 'okx_订单id',
  `strategy_type` varchar(50) NOT NULL COMMENT '策略类型',
  `period` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '策略周期',
  `inst_id` varchar(20) NOT NULL COMMENT '交易产品id',
  `side` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '买进，卖出',
  `pos_side` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '多、空',
  `tag` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL DEFAULT '' COMMENT '订单标签(时间-策略类型-产品id-周期-side-postside)',
  `detail` text NOT NULL COMMENT '下单详情',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `update_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uuid` (`uuid`),
  KEY `okx_ord_id` (`okx_ord_id`),
  KEY `inst_id` (`inst_id`),
  KEY `strategy_type` (`strategy_type`),
  KEY `period` (`period`)
) ENGINE=InnoDB AUTO_INCREMENT=14 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci COMMENT='合约下单记录表';



CREATE TABLE `strategy_config` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_type` varchar(50) NOT NULL COMMENT '策略类型',
  `inst_id` varchar(50) NOT NULL COMMENT '交易产品类型',
  `value` varchar(600) DEFAULT NULL COMMENT '配置详情',
  `time` varchar(50) NOT NULL COMMENT '交易时间段',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=3 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci COMMENT='ut_boot策略配置表';

CREATE TABLE `back_test_log` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_type` varchar(255) NOT NULL COMMENT '策略类型',
  `inst_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
  `time` varchar(255) NOT NULL,
  `win_rate` varchar(255) NOT NULL,
  `open_positions_num` int NOT NULL COMMENT '开仓次数',
  `final_fund` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
  `strategy_detail` text CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`),
  KEY `win_rate` (`win_rate`),
  KEY `final_fund` (`final_fund`),
  KEY `inst_type` (`inst_type`),
  KEY `strategy_type` (`strategy_type`)
) ENGINE=InnoDB AUTO_INCREMENT=60901 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE `back_test_detail` (
  `id` int NOT NULL AUTO_INCREMENT,
  `inst_id` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
  `time` varchar(255) NOT NULL COMMENT '周期',
  `strategy_type` varchar(255) NOT NULL COMMENT '策略类型',
  `back_test_id` int NOT NULL COMMENT '回测记录表id',
  `open_position_time` datetime NOT NULL COMMENT '开仓时间',
  `close_position_time` datetime NOT NULL COMMENT '平仓时间',
  `open_price` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '开仓时间',
  `close_price` varchar(255) NOT NULL COMMENT '平仓时间',
  `profit_loss` varchar(255) NOT NULL COMMENT '盈利/亏损金额',
  `quantity` varchar(255) NOT NULL COMMENT '开仓/平仓数量',
  `full_close` varchar(10) NOT NULL COMMENT '是否全部平仓',
  `close_type` varchar(255) NOT NULL COMMENT '平仓类型',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '时间',
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=320686 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;


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