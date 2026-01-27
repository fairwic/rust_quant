-- Full schema snapshot migrated from existing database
-- Generated on 2026-01-16 to replace manual create_table*.sql files

-- 基础表
CREATE TABLE IF NOT EXISTS `asset_classification` (
    `id` int NOT NULL AUTO_INCREMENT,
    `contentKey` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
    `isNew` int NOT NULL,
    `message` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
    `nameKey` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
    `type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL,
    `created_at` datetime DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`) USING BTREE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE IF NOT EXISTS `tickers_data` (
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
    `vol_ccy24h` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '24小时成交量（币）',
    `vol24h` varchar(255) NOT NULL COMMENT '24小时成交量（张/计价币）',
    `sod_utc0` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT 'UTC+0 时开盘价',
    `sod_utc8` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT 'UTC+8 时开盘价',
    `ts` bigint NOT NULL COMMENT '数据时间戳(ms)',
    PRIMARY KEY (`id`) USING BTREE,
    UNIQUE KEY `uk_inst_type_id` (`inst_type`, `inst_id`),
    KEY `inst_type` (`inst_type`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci COMMENT = 'Ticker 数据表';

-- 策略配置与日志
CREATE TABLE IF NOT EXISTS `strategy_config` (
    `id` int NOT NULL AUTO_INCREMENT,
    `strategy_type` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
    `inst_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易产品',
    `value` text CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci COMMENT '配置详情',
    `risk_config` varchar(2000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '风险配置',
    `time` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易周期',
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
    `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
    `kline_start_time` bigint DEFAULT NULL COMMENT '回测开始时间',
    `kline_end_time` bigint DEFAULT NULL COMMENT '回测结束时间',
    `final_fund` float NOT NULL COMMENT '回测最终资金',
    `is_deleted` smallint NOT NULL COMMENT '是否删除',
    PRIMARY KEY (`id`) USING BTREE,
    KEY `idx_inst_period` (`inst_id`, `time`) USING BTREE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_general_ci ROW_FORMAT = DYNAMIC COMMENT = '策略配置表';

CREATE TABLE IF NOT EXISTS `strategy_job_signal_log` (
    `id` int NOT NULL AUTO_INCREMENT,
    `inst_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '交易产品id',
    `time` varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '交易周期',
    `strategy_type` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '策略类型',
    `strategy_result` varchar(4000) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL COMMENT '策略结果',
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci COMMENT = '策略任务信号记录表';

CREATE TABLE IF NOT EXISTS `back_test_log` (
    `id` int NOT NULL AUTO_INCREMENT,
    `strategy_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
    `inst_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易产品',
    `time` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '周期',
    `win_rate` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '胜率',
    `open_positions_num` int NOT NULL COMMENT '开仓次数',
    `final_fund` float NOT NULL COMMENT '最终金额',
    `strategy_detail` text CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略配置',
    `risk_config_detail` varchar(1000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '风险配置',
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `profit` float DEFAULT NULL COMMENT '收益利润',
    `one_bar_after_win_rate` float DEFAULT NULL,
    `two_bar_after_win_rate` float DEFAULT NULL,
    `three_bar_after_win_rate` float DEFAULT NULL,
    `four_bar_after_win_rate` float DEFAULT NULL,
    `five_bar_after_win_rate` float DEFAULT NULL,
    `ten_bar_after_win_rate` float DEFAULT NULL,
    `kline_start_time` bigint NOT NULL COMMENT 'k线开始时间',
    `kline_end_time` bigint NOT NULL COMMENT 'k线结束时间',
    `kline_nums` int NOT NULL COMMENT '总回测k线根数',
    `sharpe_ratio` float DEFAULT NULL COMMENT '夏普比率',
    `annual_return` float DEFAULT NULL COMMENT '年化收益率',
    `total_return` float DEFAULT NULL COMMENT '绝对收益率',
    `max_drawdown` float DEFAULT NULL COMMENT '最大回撤',
    `volatility` float DEFAULT NULL COMMENT '波动率(年化)',
    PRIMARY KEY (`id`) USING BTREE,
    KEY `idx_final_fund` (`final_fund`) USING BTREE,
    KEY `idx_inst` (`inst_type`) USING BTREE,
    KEY `idx_time_fund` (`time`, `final_fund`) USING BTREE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_general_ci ROW_FORMAT = DYNAMIC;

CREATE TABLE IF NOT EXISTS `back_test_detail` (
    `id` int NOT NULL AUTO_INCREMENT,
    `back_test_id` int NOT NULL COMMENT '回测记录表id',
    `inst_id` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL,
    `time` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '周期',
    `strategy_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
    `option_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT 'long/short/close',
    `signal_open_position_time` datetime DEFAULT NULL COMMENT '信号触发时间',
    `open_position_time` datetime NOT NULL COMMENT '实际开仓时间',
    `close_position_time` datetime NOT NULL COMMENT '平仓时间',
    `open_price` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '开仓价',
    `close_price` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL COMMENT '平仓价',
    `fee` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL DEFAULT '' COMMENT '手续费',
    `profit_loss` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '盈亏',
    `quantity` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '数量',
    `full_close` varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '是否全平',
    `close_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '平仓类型',
    `signal_status` int NOT NULL COMMENT '0使用信号 -1错过 1最优价',
    `signal_value` varchar(5000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '信号详情',
    `signal_result` varchar(4000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL,
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '时间',
    `win_nums` int NOT NULL COMMENT '盈利次数',
    `loss_nums` int DEFAULT NULL COMMENT '亏损次数',
    PRIMARY KEY (`id`) USING BTREE,
    KEY `back_test_id` (`back_test_id`) USING BTREE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_general_ci ROW_FORMAT = DYNAMIC;

CREATE TABLE IF NOT EXISTS `back_test_analysis` (
    `id` int NOT NULL AUTO_INCREMENT COMMENT '分析记录',
    `back_test_id` int NOT NULL,
    `inst_id` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci NOT NULL,
    `time` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci NOT NULL,
    `option_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci NOT NULL,
    `open_position_time` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci DEFAULT NULL,
    `open_price` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci NOT NULL,
    `bars_after` int NOT NULL,
    `price_after` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci NOT NULL,
    `price_change_percent` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_520_ci NOT NULL,
    `is_profitable` tinyint NOT NULL,
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`) USING BTREE
) ENGINE = MyISAM DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_unicode_520_ci ROW_FORMAT = DYNAMIC;

CREATE TABLE IF NOT EXISTS `filtered_signal_log` (
    `id` bigint NOT NULL AUTO_INCREMENT,
    `backtest_id` bigint NOT NULL,
    `inst_id` varchar(32) NOT NULL,
    `period` varchar(10) NOT NULL,
    `signal_time` datetime NOT NULL,
    `direction` varchar(10) NOT NULL,
    `filter_reasons` json NOT NULL,
    `signal_price` decimal(20, 8) NOT NULL,
    `indicator_snapshot` json DEFAULT NULL,
    `theoretical_profit` decimal(20, 8) DEFAULT NULL,
    `theoretical_loss` decimal(20, 8) DEFAULT NULL,
    `final_pnl` decimal(20, 8) DEFAULT NULL,
    `trade_result` varchar(10) DEFAULT NULL,
    `created_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    KEY `idx_backtest` (`backtest_id`),
    KEY `idx_inst_period` (`inst_id`, `period`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- 下单与 API 配置
CREATE TABLE IF NOT EXISTS `swap_orders` (
    `id` int NOT NULL AUTO_INCREMENT,
    `strategy_id` int NOT NULL COMMENT '使用的策略id',
    `in_order_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '内部订单id 唯一',
    `out_order_id` varchar(32) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '第三方平台id',
    `strategy_type` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
    `period` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略周期',
    `inst_id` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易产品id',
    `side` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '买进/卖出',
    `pos_size` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '持仓数量',
    `pos_side` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '多/空',
    `tag` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL DEFAULT '' COMMENT '订单标签',
    `platform_type` varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '平台类型 1=okx',
    `detail` text CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '下单详情',
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
    `update_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
    PRIMARY KEY (`id`) USING BTREE,
    UNIQUE KEY `uk_in_order_id` (`in_order_id`) USING BTREE,
    UNIQUE KEY `uk_out_order_id` (`out_order_id`) USING BTREE,
    KEY `idx_inst` (`inst_id`) USING BTREE,
    KEY `idx_strategy_type` (`strategy_type`) USING BTREE,
    KEY `idx_period` (`period`) USING BTREE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_general_ci ROW_FORMAT = DYNAMIC COMMENT = '合约下单记录表';

CREATE TABLE IF NOT EXISTS `exchange_apikey_config` (
    `id` int NOT NULL AUTO_INCREMENT,
    `exchange_name` varchar(20) NOT NULL COMMENT '交易所类型',
    `api_key` varchar(100) NOT NULL COMMENT 'apikey',
    `api_secret` varchar(100) NOT NULL COMMENT 'apisecret',
    `passphrase` varchar(100) NOT NULL COMMENT 'passphrase',
    `is_sandbox` tinyint NOT NULL COMMENT '是否模拟交易 0非 1是',
    `is_enabled` tinyint NOT NULL COMMENT '是否启用 0非 1是',
    `description` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL DEFAULT '' COMMENT '描述',
    `create_user_id` int NOT NULL DEFAULT '0' COMMENT '创建者',
    `create_time` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `update_time` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    `is_deleted` tinyint NOT NULL DEFAULT '0',
    PRIMARY KEY (`id`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci COMMENT = '各交易所 api key 配置';

CREATE TABLE IF NOT EXISTS `exchange_apikey_strategy_relation` (
    `id` int NOT NULL AUTO_INCREMENT,
    `strategy_config_id` int NOT NULL,
    `api_config_id` int NOT NULL,
    `priority` tinyint NOT NULL DEFAULT '0' COMMENT '优先级',
    `is_enabled` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL DEFAULT '0' COMMENT '是否启用',
    `is_deleted` tinyint NOT NULL DEFAULT '0',
    PRIMARY KEY (`id`),
    KEY `idx_strategy_priority` (
        `strategy_config_id`,
        `priority`,
        `is_enabled`
    ),
    KEY `idx_api_config` (`api_config_id`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

-- 资金费率
CREATE TABLE IF NOT EXISTS `funding_rates` (
    `id` bigint NOT NULL AUTO_INCREMENT COMMENT '自增主键',
    `inst_id` varchar(32) NOT NULL COMMENT '产品ID',
    `funding_time` bigint NOT NULL COMMENT '资金费时间戳',
    `funding_rate` varchar(32) NOT NULL COMMENT '资金费率',
    `method` varchar(20) NOT NULL COMMENT '收付逻辑: current_period/next_period',
    `next_funding_rate` varchar(32) DEFAULT NULL COMMENT '下一期预测资金费率',
    `next_funding_time` bigint DEFAULT NULL COMMENT '下一期资金费时间戳',
    `min_funding_rate` varchar(32) DEFAULT NULL COMMENT '资金费率下限',
    `max_funding_rate` varchar(32) DEFAULT NULL COMMENT '资金费率上限',
    `sett_funding_rate` varchar(32) DEFAULT NULL COMMENT '结算资金费率',
    `sett_state` varchar(20) DEFAULT NULL COMMENT '结算状态',
    `premium` varchar(32) DEFAULT NULL COMMENT '溢价指数',
    `ts` bigint NOT NULL COMMENT '数据更新时间戳',
    `realized_rate` varchar(32) DEFAULT NULL COMMENT '实际资金费率',
    `interest_rate` varchar(32) DEFAULT NULL COMMENT '利率',
    `created_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    UNIQUE KEY `uk_inst_time` (`inst_id`, `funding_time`),
    KEY `idx_funding_time` (`funding_time`),
    KEY `idx_ts` (`ts`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci COMMENT = '资金费率表';

-- K线历史表（各品种/周期分表）
CREATE TABLE IF NOT EXISTS `btc-usdt-swap_candles_4h` (
    `id` int NOT NULL AUTO_INCREMENT,
    `ts` bigint NOT NULL COMMENT '开始时间(ms)',
    `o` varchar(20) NOT NULL COMMENT '开盘价',
    `h` varchar(20) NOT NULL COMMENT '最高价',
    `l` varchar(20) NOT NULL COMMENT '最低价',
    `c` varchar(20) NOT NULL COMMENT '收盘价',
    `vol` varchar(20) NOT NULL COMMENT '交易量(张)',
    `vol_ccy` varchar(50) NOT NULL COMMENT '交易量(币)',
    `confirm` varchar(20) NOT NULL COMMENT 'K线状态',
    `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at` datetime DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    UNIQUE KEY `uk_ts` (`ts` DESC) USING BTREE,
    KEY `idx_vol_ccy` (`vol_ccy` DESC)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;

CREATE TABLE IF NOT EXISTS `eth-usdt-swap_candles_1dutc` LIKE `btc-usdt-swap_candles_4h`;

CREATE TABLE IF NOT EXISTS `eth-usdt-swap_candles_1h` LIKE `btc-usdt-swap_candles_4h`;

CREATE TABLE IF NOT EXISTS `eth-usdt-swap_candles_4h` LIKE `btc-usdt-swap_candles_4h`;

CREATE TABLE IF NOT EXISTS `eth-usdt-swap_candles_5m` LIKE `btc-usdt-swap_candles_4h`;

CREATE TABLE IF NOT EXISTS `sol-usdt-swap_candles_4h` LIKE `btc-usdt-swap_candles_4h`;

-- INSERT INTO
--     `test`.`strategy_config` (
--         `id`,
--         `strategy_type`,
--         `inst_id`,
--         `value`,
--         `risk_config`,
--         `time`,
--         `created_at`,
--         `updated_at`,
--         `kline_start_time`,
--         `kline_end_time`,
--         `final_fund`,
--         `is_deleted`
--     )
-- VALUES (
--         1,
--         'Vegas',
--         'ETH-USDT-SWAP',
--         '{\"period\":\"4H\",\"min_k_line_num\":3600,\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"volume_signal\":{\"volume_bar_num\":4,\"volume_increase_ratio\":2.5,\"volume_decrease_ratio\":2.5,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995,\"is_open\":true},\"rsi_signal\":{\"rsi_length\":16,\"rsi_oversold\":14.0,\"rsi_overbought\":86.0,\"is_open\":true},\"bolling_signal\":{\"period\":12,\"multiplier\":2.0,\"is_open\":true,\"consecutive_touch_times\":4},\"signal_weights\":{\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",1.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",0.9],[\"MarketStructure\",0.0],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3],[\"FakeBreakout\",0.0]],\"min_total_weight\":2.0},\"engulfing_signal\":{\"is_engulfing\":true,\"body_ratio\":0.4,\"is_open\":true},\"kline_hammer_signal\":{\"up_shadow_ratio\":0.6,\"down_shadow_ratio\":0.6},\"leg_detection_signal\":{\"size\":7,\"is_open\":true},\"market_structure_signal\":{\"swing_length\":12,\"internal_length\":2,\"swing_threshold\":0.015,\"internal_threshold\":0.015,\"enable_swing_signal\":false,\"enable_internal_signal\":true,\"is_open\":true},\"fair_value_gap_signal\":{\"threshold_multiplier\":1.0,\"auto_threshold\":true,\"is_open\":false},\"premium_discount_signal\":{\"premium_threshold\":0.05,\"discount_threshold\":0.05,\"lookback\":20,\"is_open\":false},\"fake_breakout_signal\":null,\"range_filter_signal\":{\"bb_width_threshold\":0.03,\"tp_kline_ratio\":0.6,\"is_open\":true},\"extreme_k_filter_signal\":{\"is_open\":true,\"min_body_ratio\":0.65,\"min_move_pct\":0.01,\"min_cross_ema_count\":2},\"chase_confirm_config\":{\"enabled\":true,\"long_threshold\":0.18,\"short_threshold\":0.10,\"pullback_touch_threshold\":0.05,\"min_body_ratio\":0.5,\"close_to_ema_threshold\":0.0025,\"tight_stop_loss_ratio\":0.998}}',
--         '{\"max_loss_percent\": 0.04, \"atr_take_profit_ratio\": 3.0, \"is_one_k_line_diff_stop_loss\": false, \"is_used_signal_k_line_stop_loss\": false, \"is_counter_trend_pullback_take_profit\": false, \"is_move_stop_open_price_when_touch_price\": false}',
--         '4H',
--         '2025-10-10 18:04:33',
--         '2026-01-09 09:35:05',
--         1577232000000,
--         1760083200000,
--         4352010,
--         0
--     );
