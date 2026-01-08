-- 经济日历事件表
-- 存储 OKX 经济日历数据，用于分析重要经济事件对市场的影响
DROP TABLE IF EXISTS `economic_events`;
CREATE TABLE IF NOT EXISTS `economic_events` (
    `id` BIGINT NOT NULL AUTO_INCREMENT COMMENT '自增主键',
    `calendar_id` VARCHAR(64) NOT NULL COMMENT 'OKX 经济日历ID',
    `event_time` BIGINT NOT NULL COMMENT '计划发布时间 (Unix时间戳毫秒)',
    `region` VARCHAR(32) NOT NULL COMMENT '事件区域 (如 US, EU, CN)',
    `category` VARCHAR(128) NOT NULL COMMENT '事件类别',
    `event` VARCHAR(256) NOT NULL COMMENT '事件名称/指标',
    `ref_date` VARCHAR(64) NOT NULL COMMENT '事件指向日期 (参考期间)',
    `actual` VARCHAR(64) NULL COMMENT '实际值',
    `previous` VARCHAR(64) NULL COMMENT '前值',
    `forecast` VARCHAR(64) NULL COMMENT '预期值',
    `importance` TINYINT NOT NULL DEFAULT 1 COMMENT '重要性: 1=低, 2=中, 3=高',
    `updated_time` BIGINT NOT NULL COMMENT '数据最后更新时间 (Unix时间戳毫秒)',
    `prev_initial` VARCHAR(64) NULL COMMENT '初始前值',
    `currency` VARCHAR(16) NOT NULL COMMENT '货币',
    `unit` VARCHAR(32) NULL COMMENT '单位',
    `created_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
    `updated_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
    PRIMARY KEY (`id`),
    UNIQUE KEY `uk_calendar_id` (`calendar_id`),
    INDEX `idx_event_time` (`event_time`),
    INDEX `idx_importance` (`importance`),
    INDEX `idx_region` (`region`),
    INDEX `idx_event_time_importance` (`event_time`, `importance`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='经济日历事件表';

