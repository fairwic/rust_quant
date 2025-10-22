
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

