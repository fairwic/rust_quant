
CREATE TABLE `tickers_data` (
  `id` int NOT NULL AUTO_INCREMENT,
  `inst_type` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '产品类型',
  `inst_id` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '产品ID',
  `last` varchar(255) NOT NULL COMMENT '最新成交价',
  `last_sz` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '最新成交的数量',
  `ask_px` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '卖一价',
  `ask_sz` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '卖一价对应的数量',
  `bid_px` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '买一价',
  `bid_sz` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '买一价对应的数量',
  `open24h` varchar(255) NOT NULL COMMENT '24小时开盘价',
  `high24h` varchar(255) NOT NULL COMMENT '24小时最高价',
  `low24h` varchar(255) NOT NULL COMMENT '24小时最低价',
  `vol_ccy24h` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT '24小时成交量，以币为单位。如果是衍生品合约，数值为交易货币的数量。如果是币币/币币杠杆，数值为计价货币的数量',
  `vol24h` varchar(255) NOT NULL COMMENT '24小时成交量，以张为单位。如果是衍生品合约，数值为合约的张数。如果是币币/币币杠杆，数值为交易货币的数量',
  `sod_utc0` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT 'UTC+0 时开盘价',
  `sod_utc8` varchar(255) CHARACTER SET utf8mb4   NOT NULL COMMENT 'UTC+8 时开盘价',
  `ts` bigint NOT NULL COMMENT 'ticker数据产生时间，Unix时间戳的毫秒数格式，如 1597026383085',
  PRIMARY KEY (`id`) USING BTREE,
  UNIQUE KEY `inst_type_2` (`inst_type`,`inst_id`),
  KEY `inst_type` (`inst_type`)
) ENGINE=InnoDB AUTO_INCREMENT=6808 DEFAULT CHARSET=utf8mb4   COMMENT='Ticker 数据表';


CREATE TABLE `strategy_job_signal_log` (
  `id` int NOT NULL AUTO_INCREMENT,
  `inst_id` varchar(50) CHARACTER SET utf8mb4   NOT NULL COMMENT '交易产品id',
  `time` varchar(10) CHARACTER SET utf8mb4   NOT NULL COMMENT '交易周期',
  `strategy_type` varchar(50) CHARACTER SET utf8mb4   NOT NULL COMMENT '策略类型',
  `strategy_result` varchar(4000) CHARACTER SET utf8mb4   NOT NULL COMMENT '策略结果',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=307 DEFAULT CHARSET=utf8mb4   COMMENT='策略任务信号记录表';

CREATE TABLE `swap_orders` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_id` int NOT NULL COMMENT '使用的策略id',
  `in_order_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '内部订单id 唯一',
  `out_order_id` varchar(32) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '第三方平台id',
  `strategy_type` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
  `period` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略周期',
  `inst_id` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易产品id',
  `side` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '买进，卖出',
  `pos_size` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '持仓数量',
  `pos_side` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '多、空',
  `tag` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL DEFAULT '' COMMENT '订单标签(时间-策略类型-产品id-周期-side-postside)',
  `platform_type` varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '下单的平台类型1okx',
  `detail` text CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '下单详情',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `update_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  PRIMARY KEY (`id`) USING BTREE,
  UNIQUE KEY `cl_ord_id` (`in_order_id`) USING BTREE,
  UNIQUE KEY `out_order_id` (`out_order_id`) USING BTREE,
  KEY `inst_id` (`inst_id`) USING BTREE,
  KEY `strategy_type` (`strategy_type`) USING BTREE,
  KEY `period` (`period`) USING BTREE
) ENGINE=InnoDB AUTO_INCREMENT=27 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci ROW_FORMAT=DYNAMIC COMMENT='合约下单记录表';


CREATE TABLE `strategy_config` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_type` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
  `inst_id` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易产品类型',
  `value` varchar(2000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL COMMENT '配置详情',
  `risk_config` varchar(2000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '风险配置',
  `time` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易时间段',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  `kline_start_time` bigint DEFAULT NULL COMMENT '回测开始时间',
  `kline_end_time` bigint DEFAULT NULL COMMENT '回测结束时间',
  `final_fund` float NOT NULL COMMENT '回测最终资金',
  `is_deleted` smallint NOT NULL COMMENT '是否删除',
  PRIMARY KEY (`id`) USING BTREE,
  KEY `inst_id` (`inst_id`,`time`) USING BTREE
) ENGINE=InnoDB AUTO_INCREMENT=11 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci ROW_FORMAT=DYNAMIC COMMENT='ut_boot策略配置表';



CREATE TABLE `back_test_log` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
  `inst_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易产品',
  `time` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '周期',
  `win_rate` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '胜率',
  `open_positions_num` int NOT NULL COMMENT '开仓次数',
  `final_fund` float NOT NULL COMMENT '最终金额',
  `strategy_detail` text CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略的配置',
  `risk_config_detail` varchar(1000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '风险控制的配置',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `profit` float DEFAULT NULL COMMENT '收益利润',
  `one_bar_after_win_rate` float DEFAULT NULL COMMENT '第1根线是盈利的状态占总开仓次数的比例',
  `two_bar_after_win_rate` float DEFAULT NULL COMMENT '第2根线是盈利的状态占总开仓次数的比例',
  `three_bar_after_win_rate` float DEFAULT NULL COMMENT '第3根线是盈利的状态占总开仓次数的比例',
  `four_bar_after_win_rate` float DEFAULT NULL COMMENT '第4根线是盈利的状态占总开仓次数的比例',
  `five_bar_after_win_rate` float DEFAULT NULL COMMENT '第5根线是盈利的状态占总开仓次数的比例',
  `ten_bar_after_win_rate` float DEFAULT NULL COMMENT '第10根线是盈利的状态占总开仓次数的比例',
  `kline_start_time` bigint NOT NULL COMMENT 'k线开始时间',
  `kline_end_time` bigint NOT NULL COMMENT 'k线结束时间',
  `kline_nums` int NOT NULL COMMENT '总回测k线根数',
  `sharpe_ratio` float DEFAULT NULL COMMENT '夏普比率',
  `annual_return` float DEFAULT NULL COMMENT '年化收益率',
  `total_return` float DEFAULT NULL COMMENT '绝对收益率',
  `max_drawdown` float DEFAULT NULL COMMENT '最大回撤',
  `volatility` float DEFAULT NULL COMMENT '波动率(年化)',
  PRIMARY KEY (`id`) USING BTREE,
  KEY `final_fund` (`final_fund`) USING BTREE,
  KEY `inst_type` (`inst_type`) USING BTREE,
  KEY `time` (`time`,`final_fund`) USING BTREE
) ENGINE=InnoDB AUTO_INCREMENT=44700 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci ROW_FORMAT=DYNAMIC;



CREATE TABLE `back_test_analysis` (
  `id` int NOT NULL AUTO_INCREMENT COMMENT '    pub id: Option<i32>,\r\n    pub back_test_id: i32,\r\n    pub inst_id: String,\r\n    pub time: String,\r\n    pub option_type: String,\r\n    pub open_position_time: String,\r\n    pub open_price: String,\r\n    pub bars_after: i32,\r\n    pub price_after: String,\r\n    pub price_change_percent: String,\r\n    pub is_profitable: i32,\r\n    pub created_at: Option<String>,',
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
) ENGINE=MyISAM DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_520_ci ROW_FORMAT=DYNAMIC;


CREATE TABLE `back_test_detail` (
  `id` int NOT NULL AUTO_INCREMENT,
  `back_test_id` int NOT NULL COMMENT '回测记录表id',
  `inst_id` varchar(20) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL,
  `time` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '周期',
  `strategy_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '策略类型',
  `option_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT 'long 开多，short开空 close平仓',
  `signal_open_position_time` datetime DEFAULT NULL COMMENT '信号触发时间',
  `open_position_time` datetime NOT NULL COMMENT '实际开仓时间',
  `close_position_time` datetime NOT NULL COMMENT '平仓时间',
  `open_price` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '实际开仓价格',
  `close_price` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL COMMENT '实际平仓时间',
  `fee` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL DEFAULT '' COMMENT '手续费',
  `profit_loss` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '盈利/亏损金额',
  `quantity` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '开仓/平仓数量',
  `full_close` varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '是否全部平仓',
  `close_type` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '平仓类型',
  `signal_status` int NOT NULL COMMENT '0使用信号正常 -1信号错过 1使用信号的最优价格',
  `signal_value` varchar(5000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '此次操作依赖的信号详情',
  `signal_result` varchar(4000) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL,
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '时间',
  `win_nums` int NOT NULL COMMENT '盈利金额数量',
  `loss_nums` int DEFAULT NULL COMMENT '亏损金额数量',
  PRIMARY KEY (`id`) USING BTREE,
  KEY `back_test_id` (`back_test_id`) USING BTREE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci ROW_FORMAT=DYNAMIC;



CREATE TABLE `asset_classification` (
  `id` int NOT NULL AUTO_INCREMENT,
  `contentKey` varchar(255) CHARACTER SET utf8mb4   NOT NULL,
  `isNew` int NOT NULL,
  `message` varchar(255) CHARACTER SET utf8mb4   NOT NULL,
  `nameKey` varchar(255) CHARACTER SET utf8mb4   NOT NULL,
  `type` varchar(255) CHARACTER SET utf8mb4   NOT NULL,
  `created_at` datetime DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`) USING BTREE
) ENGINE=InnoDB AUTO_INCREMENT=40 DEFAULT CHARSET=utf8mb4  ;


INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (1, 'asset_db_asset_classficiation_desc_hot_currency', 0, '主流币', 'asset_db_text_main_currency', 'Top', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (4, 'asset_db_asset_classficiation_desc_hot_defi_currency', 0, 'DeFi', 'asset_db_text_defi', 'DeFi', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (5, 'asset_db_asset_classficiation_desc_hot_nft_currency', 0, 'NFT 生态', 'asset_db_text_nft', 'NFT', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (6, 'asset_db_asset_classficiation_desc_storage', 0, '存储项目', 'asset_db_text_storage', 'Storage', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (9, 'asset_db_asset_classficiation_desc_layer_2', 0, 'Layer 2', 'asset_db_text_layer2', 'Layer 2', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (10, 'asset_db_asset_classficiation_desc_meme', 0, 'Meme', 'asset_db_text_meme', 'Meme', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (11, 'asset_db_asset_classficiation_desc_gamefi', 0, '游戏代币', 'asset_db_text_gamefi', 'Gaming', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (12, 'asset_db_asset_classficiation_desc_publicchain', 0, 'Layer 1', 'asset_db_text_blockchain', 'Layer 1', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (14, 'asset_db_asset_classficiation_desc_fantoken', 0, '粉丝代币', 'asset_db_text_fantoken', 'Fan Tokens', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (17, 'asset_db_asset_classficiation_desc_pow', 0, 'PoW', 'asset_db_text_pow', 'Proof of Work', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (18, 'asset_db_asset_classficiation_desc_ai', 0, '人工智能与大数据', 'asset_db_text_ai', 'AI Big Data', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (30, 'asset_db_asset_classficiation_desc_inscription', 0, '铭文', 'asset_db_text_inscription', 'Inscriptions', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (32, 'asset_db_asset_classficiation_desc_depin', 0, 'DePIN', 'asset_db_text_depin', 'DePIN', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (33, 'asset_db_asset_classficiation_desc_dex', 0, 'DEX', 'asset_db_text_dex', 'DEX', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (34, 'asset_db_asset_classficiation_desc_governance', 0, '治理代币', 'asset_db_text_governance', 'Governance', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (35, 'asset_db_asset_classficiation_desc_infrastructure', 0, '基础设施', 'asset_db_text_infrastructure', 'Infrastructure', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (36, 'asset_db_asset_classficiation_desc_lend_borrow', 0, '借贷项目', 'asset_db_text_lend_borrow', 'Lending Borrowing', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (37, 'asset_db_asset_classficiation_desc_metaverse', 0, '元宇宙', 'asset_db_text_metaverse', 'Metaverse', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (38, 'asset_db_asset_classficiation_desc_others', 0, '其他分类', 'asset_db_text_others', 'Others', NULL);
INSERT INTO `test`.`asset_classification` (`id`, `contentKey`, `isNew`, `message`, `nameKey`, `type`, `created_at`) VALUES (39, 'asset_db_asset_classficiation_desc_yieldfarm', 0, '流动性挖矿', 'asset_db_text_yieldfarm', 'Yield Farming', NULL);

CREATE TABLE `exchange_apikey_config` (
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
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=2 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci COMMENT='各交易所api key 配置';



CREATE TABLE `exchange_apikey_strategy_relation` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_config_id` int NOT NULL,
  `api_key_config_id` int NOT NULL,
  `priority` tinyint NOT NULL DEFAULT '0' COMMENT '优先级',
  `is_enabled` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci NOT NULL DEFAULT '0' COMMENT '是否启用',
  PRIMARY KEY (`id`),
  KEY `strategy_config_id` (`strategy_config_id`,`priority`,`is_enabled`),
  KEY `api_key_config_id` (`api_key_config_id`)
) ENGINE=InnoDB AUTO_INCREMENT=2 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;



INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (1, 'Vegas', 'BTC-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":2.5,\"period\":13},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.6,\"up_shadow_ratio\":0.6},\"min_k_line_num\":3600,\"rsi_signal\":{\"is_open\":true,\"rsi_length\":9,\"rsi_overbought\":85.0,\"rsi_oversold\":15.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",2.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_force_dependent\":false,\"is_open\":true,\"volume_bar_num\":6,\"volume_decrease_ratio\":2.0,\"volume_increase_ratio\":2.4},\"period\":\"1H\"}', '{\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.02,\"is_take_profit\":true}', '1H', '2025-04-05 17:23:36', '2025-09-19 01:24:42', 1679000400000, 1750996800000, 1230.93, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (2, 'Vegas', 'BTC-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":2.0,\"period\":12},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.85,\"up_shadow_ratio\":0.85},\"min_k_line_num\":3600,\"rsi_signal\":{\"is_open\":true,\"rsi_length\":20,\"rsi_overbought\":88.0,\"rsi_oversold\":15.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",2.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_force_dependent\":false,\"is_open\":true,\"volume_bar_num\":6,\"volume_decrease_ratio\":1.6,\"volume_increase_ratio\":1.9},\"period\":\"4H\"}', '{\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.02,\"is_take_profit\":true}', '4H', '2025-06-27 12:07:42', '2025-09-19 01:24:48', 1576468800000, 1750982400000, 1847.51, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (5, 'Vegas', 'BTC-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":2.0,\"period\":10},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.9,\"up_shadow_ratio\":0.9},\"min_k_line_num\":3600,\"period\":\"1Dutc\",\"rsi_signal\":{\"is_open\":true,\"rsi_length\":12,\"rsi_overbought\":90.0,\"rsi_oversold\":25.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",1.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_open\":true,\"volume_bar_num\":5,\"volume_decrease_ratio\":1.9000000000000001,\"volume_increase_ratio\":1.9000000000000001}}', '{\"is_one_k_line_diff_stop_loss\":false,\"take_profit_ratio\":0.0,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.04}', '1Dutc', '2025-07-01 14:30:26', '2025-10-28 03:35:55', 1577836800000, 1760313600000, 9128.49, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (7, 'Vegas', 'ETH-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":3.0,\"period\":15},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.8,\"up_shadow_ratio\":0.8},\"min_k_line_num\":3600,\"rsi_signal\":{\"is_open\":true,\"rsi_length\":16,\"rsi_overbought\":86.0,\"rsi_oversold\":15.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",2.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_force_dependent\":false,\"is_open\":true,\"volume_bar_num\":4,\"volume_decrease_ratio\":2.2,\"volume_increase_ratio\":2.2},\"period\":\"1H\"}', '{\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.03,\"is_take_profit\":true}', '1H', '2025-07-31 00:22:01', '2025-10-13 14:59:44', 1577836800000, 1753747200000, 1, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (8, 'Vegas', 'ETH-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":3.0,\"period\":10},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.9,\"up_shadow_ratio\":0.9},\"min_k_line_num\":3600,\"rsi_signal\":{\"is_open\":true,\"rsi_length\":20,\"rsi_overbought\":90.0,\"rsi_oversold\":20.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",2.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_force_dependent\":false,\"is_open\":true,\"volume_bar_num\":6,\"volume_decrease_ratio\":2.0,\"volume_increase_ratio\":2.0},\"period\":\"1Dutc\"}', '{\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.03,\"is_take_profit\":true}', '1Dutc', '2025-07-31 10:11:34', '2025-10-13 14:59:46', 1577836800000, 1753747200000, 3924.64, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (9, 'Vegas', 'SOL-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":2.5,\"period\":13},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.6,\"up_shadow_ratio\":0.6},\"min_k_line_num\":3600,\"rsi_signal\":{\"is_open\":true,\"rsi_length\":9,\"rsi_overbought\":85.0,\"rsi_oversold\":15.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",2.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_force_dependent\":false,\"is_open\":true,\"volume_bar_num\":6,\"volume_decrease_ratio\":2.0,\"volume_increase_ratio\":2.4},\"period\":\"1H\"}', '{\"is_one_k_line_diff_stop_loss\":false,\"is_take_profit\":true,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.03}', '1H', '2025-09-08 23:07:41', '2025-09-19 01:25:44', 1685329200000, 1757325600000, 2340890, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (10, 'Vegas', 'ETH-USDT-SWAP', '{\"bolling_signal\":{\"consecutive_touch_times\":4,\"is_open\":true,\"multiplier\":3.0,\"period\":10},\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"is_open\":true,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995},\"engulfing_signal\":{\"body_ratio\":0.4,\"is_engulfing\":true,\"is_open\":true},\"kline_hammer_signal\":{\"down_shadow_ratio\":0.9,\"up_shadow_ratio\":0.9},\"min_k_line_num\":3600,\"rsi_signal\":{\"is_open\":true,\"rsi_length\":20,\"rsi_overbought\":90.0,\"rsi_oversold\":20.0},\"signal_weights\":{\"min_total_weight\":2.0,\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",2.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]]},\"volume_signal\":{\"is_force_dependent\":false,\"is_open\":true,\"volume_bar_num\":6,\"volume_decrease_ratio\":2.0,\"volume_increase_ratio\":2.0},\"period\":\"1Dutc\"}', '{\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.03,\"is_take_profit\":true}', '5m', '2025-07-31 10:11:34', '2025-10-13 14:59:48', 1577836800000, 1753747200000, 3924.64, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (11, 'Vegas', 'ETH-USDT-SWAP', '{\"period\":\"4H\",\"min_k_line_num\":3600,\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"volume_signal\":{\"volume_bar_num\":4,\"volume_increase_ratio\":2.0,\"volume_decrease_ratio\":2.0,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995,\"is_open\":true},\"rsi_signal\":{\"rsi_length\":16,\"rsi_oversold\":15.0,\"rsi_overbought\":90.0,\"is_open\":true},\"bolling_signal\":{\"period\":12,\"multiplier\":2.0,\"is_open\":true,\"consecutive_touch_times\":4},\"signal_weights\":{\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",1.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",1.2],[\"MarketStructure\",1.8],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3]],\"min_total_weight\":2.0},\"engulfing_signal\":{\"is_engulfing\":true,\"body_ratio\":0.4,\"is_open\":true},\"kline_hammer_signal\":{\"up_shadow_ratio\":0.7,\"down_shadow_ratio\":0.7}}', '{\"atr_take_profit_ratio\":0.0,\"is_counter_trend_pullback_take_profit\":false,\"is_move_stop_open_price_when_touch_price\":true,\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.05}', '4H', '2025-10-10 18:04:33', '2025-12-04 14:58:46', 1577232000000, 1760083200000, 4352010, 1);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (12, 'Nwe', 'ETH-USDT-SWAP', '{\"period\":\"5m\",\"stc_fast_length\":23,\"stc_slow_length\":50,\"stc_cycle_length\":10,\"stc_d1_length\":3,\"stc_d2_length\":3,\"stc_overbought\":75.0,\"stc_oversold\":25.0,\"atr_period\":11,\"atr_multiplier\":0.6,\"nwe_period\":6,\"nwe_multi\":2.6,\"volume_bar_num\":3,\"volume_ratio\":0.8,\"min_k_line_num\":500,\"k_line_hammer_shadow_ratio\":0.65}', '{\"atr_take_profit_ratio\":0.0,\"fixed_signal_kline_take_profit_ratio\":0.0,\"is_move_stop_open_price_when_touch_price\":true,\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":true,\"max_loss_percent\":0.05,\"is_counter_trend_pullback_take_profit\":true}', '5m', '2025-10-20 07:12:48', '2025-12-04 14:59:15', 1755210600000, 1761210300000, 146.609, 0);
INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (13, 'Nwe', 'ETH-USDT-SWAP', '{\"period\":\"5m\",\"stc_fast_length\":23,\"stc_slow_length\":50,\"stc_cycle_length\":10,\"stc_d1_length\":3,\"stc_d2_length\":3,\"stc_overbought\":75.0,\"stc_oversold\":25.0,\"atr_period\":6,\"atr_multiplier\":0.6,\"nwe_period\":6,\"nwe_multi\":2.2,\"volume_bar_num\":3,\"volume_ratio\":0.8,\"min_k_line_num\":500,\"k_line_hammer_shadow_ratio\":0.65}', '{\"is_move_stop_open_price_when_touch_price\":true,\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":false,\"max_loss_percent\":0.03,\"take_profit_ratio\":0.5}', '15m', '2025-11-18 10:15:02', '2025-11-18 11:31:31', 1737385200000, 1763394300000, 128.56, 1);