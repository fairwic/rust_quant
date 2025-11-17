-- 交易所API配置表
CREATE TABLE `exchange_api_config` (
  `id` int NOT NULL AUTO_INCREMENT,
  `exchange_name` varchar(50) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT '交易所名称（okx）',
  `api_key` varchar(200) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT 'API Key',
  `api_secret` varchar(200) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL COMMENT 'API Secret',
  `passphrase` varchar(200) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL COMMENT 'Passphrase（OKX需要）',
  `is_sandbox` tinyint(1) NOT NULL DEFAULT 0 COMMENT '是否沙箱环境',
  `is_enabled` tinyint(1) NOT NULL DEFAULT 1 COMMENT '是否启用',
  `description` varchar(500) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci DEFAULT NULL COMMENT '描述',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  `is_deleted` smallint NOT NULL DEFAULT 0 COMMENT '是否删除',
  PRIMARY KEY (`id`) USING BTREE,
  KEY `exchange_name` (`exchange_name`, `is_enabled`, `is_deleted`) USING BTREE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci ROW_FORMAT=DYNAMIC COMMENT='交易所API配置表';

-- 策略与API配置关联表（多对多）
CREATE TABLE `strategy_api_config` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_config_id` int NOT NULL COMMENT '策略配置ID',
  `api_config_id` int NOT NULL COMMENT 'API配置ID',
  `priority` int NOT NULL DEFAULT 0 COMMENT '优先级（数字越小优先级越高）',
  `is_enabled` tinyint(1) NOT NULL DEFAULT 1 COMMENT '是否启用',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  `is_deleted` smallint NOT NULL DEFAULT 0 COMMENT '是否删除',
  PRIMARY KEY (`id`) USING BTREE,
  UNIQUE KEY `strategy_api_unique` (`strategy_config_id`, `api_config_id`, `is_deleted`) USING BTREE,
  KEY `strategy_config_id` (`strategy_config_id`, `is_enabled`, `is_deleted`) USING BTREE,
  KEY `api_config_id` (`api_config_id`, `is_enabled`, `is_deleted`) USING BTREE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci ROW_FORMAT=DYNAMIC COMMENT='策略与API配置关联表';

