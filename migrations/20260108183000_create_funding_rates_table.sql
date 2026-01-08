DROP TABLE IF EXISTS `funding_rates`;
CREATE TABLE IF NOT EXISTS `funding_rates` (
    `id` BIGINT NOT NULL AUTO_INCREMENT COMMENT '自增主键',
    `inst_id` VARCHAR(32) NOT NULL COMMENT '产品ID',
    `funding_time` BIGINT NOT NULL COMMENT '资金费时间戳',
    `funding_rate` VARCHAR(32) NOT NULL COMMENT '资金费率',
    `method` VARCHAR(20) NOT NULL COMMENT '收付逻辑: current_period/next_period',
    `next_funding_rate` VARCHAR(32) NULL COMMENT '下一期预测资金费率',
    `next_funding_time` BIGINT NULL COMMENT '下一期资金费时间戳',
    `min_funding_rate` VARCHAR(32) NULL COMMENT '资金费率下限',
    `max_funding_rate` VARCHAR(32) NULL COMMENT '资金费率上限',
    `sett_funding_rate` VARCHAR(32) NULL COMMENT '结算资金费率',
    `sett_state` VARCHAR(20) NULL COMMENT '结算状态',
    `premium` VARCHAR(32) NULL COMMENT '溢价指数',
    `ts` BIGINT NOT NULL COMMENT '数据更新时间戳',
    `realized_rate` VARCHAR(32) NULL COMMENT '实际资金费率',
    `interest_rate` VARCHAR(32) NULL COMMENT '利率',
    `created_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    `updated_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    UNIQUE KEY `uk_inst_time` (`inst_id`, `funding_time`),
    INDEX `idx_funding_time` (`funding_time`),
    INDEX `idx_ts` (`ts`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='资金费率表';
