CREATE TABLE IF NOT EXISTS `dynamic_config_log` (
    `id` bigint NOT NULL AUTO_INCREMENT,
    `backtest_id` bigint NOT NULL,
    `inst_id` varchar(32) NOT NULL,
    `period` varchar(10) NOT NULL,
    `kline_time` datetime NOT NULL,
    `adjustments` json NOT NULL,
    `config_snapshot` json DEFAULT NULL,
    `created_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`id`),
    KEY `idx_backtest` (`backtest_id`),
    KEY `idx_inst_period` (`inst_id`, `period`),
    KEY `idx_kline_time` (`kline_time`)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4 COLLATE = utf8mb4_0900_ai_ci;
