-- 策略信号日志表
CREATE TABLE IF NOT EXISTS `strategy_signal_log` (
    `id` BIGINT AUTO_INCREMENT PRIMARY KEY,
    `inst_id` VARCHAR(50) NOT NULL COMMENT '交易对',
    `period` VARCHAR(10) NOT NULL COMMENT '周期',
    `strategy_type` VARCHAR(50) NOT NULL COMMENT '策略类型',
    `signal_result` TEXT COMMENT '信号结果（JSON格式）',
    `created_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
    
    INDEX `idx_inst_period` (`inst_id`, `period`),
    INDEX `idx_strategy_type` (`strategy_type`),
    INDEX `idx_created_at` (`created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='策略信号日志表';

