
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
    open_positions_num DESC