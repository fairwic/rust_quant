-- 重构 market_anomalies 表: 每个 symbol 一条记录 (UPSERT 模式)
-- DROP 旧表并重建

DROP TABLE IF EXISTS market_anomalies;

CREATE TABLE market_anomalies (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL UNIQUE,
    current_rank INT NOT NULL,
    rank_15m_ago INT,
    rank_6h_ago INT,
    rank_24h_ago INT,
    delta_15m INT,
    delta_6h INT,
    delta_24h INT,
    volume_24h DECIMAL(30, 8),
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    status VARCHAR(20) NOT NULL DEFAULT 'ACTIVE',
    INDEX idx_status (status),
    INDEX idx_current_rank (current_rank)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
