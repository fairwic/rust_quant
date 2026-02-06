-- Trading audit/OMS/portfolio tables

CREATE TABLE IF NOT EXISTS strategy_run (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    strategy_id VARCHAR(64) NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    period VARCHAR(16) NOT NULL,
    start_at TIMESTAMP NULL,
    end_at TIMESTAMP NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'RUNNING',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uk_run_id (run_id),
    KEY idx_strategy_inst (strategy_id, inst_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS signal_snapshot_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    kline_ts BIGINT NOT NULL,
    filtered TINYINT NOT NULL DEFAULT 0,
    filter_reasons JSON NULL,
    signal_json JSON NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY idx_run_ts (run_id, kline_ts)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS risk_decision_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    kline_ts BIGINT NOT NULL,
    decision VARCHAR(16) NOT NULL,
    reason VARCHAR(255) DEFAULT NULL,
    risk_json JSON NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY idx_run_ts (run_id, kline_ts)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS order_decision_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    kline_ts BIGINT NOT NULL,
    side VARCHAR(16) NOT NULL,
    size DECIMAL(30, 10) NOT NULL,
    price DECIMAL(30, 10) NOT NULL,
    decision_json JSON NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY idx_run_ts (run_id, kline_ts)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS orders (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    strategy_id VARCHAR(64) NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    side VARCHAR(16) NOT NULL,
    qty DECIMAL(30, 10) NOT NULL,
    price DECIMAL(30, 10) NOT NULL,
    status VARCHAR(16) NOT NULL,
    client_order_id VARCHAR(64) DEFAULT NULL,
    exchange_order_id VARCHAR(64) DEFAULT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NULL,
    KEY idx_run (run_id),
    KEY idx_inst (inst_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS order_state_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    order_id BIGINT NOT NULL,
    from_state VARCHAR(16) NOT NULL,
    to_state VARCHAR(16) NOT NULL,
    reason VARCHAR(255) DEFAULT NULL,
    ts BIGINT NOT NULL,
    KEY idx_order (order_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS positions (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    strategy_id VARCHAR(64) NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    side VARCHAR(16) NOT NULL,
    qty DECIMAL(30, 10) NOT NULL,
    avg_price DECIMAL(30, 10) NOT NULL,
    unrealized_pnl DECIMAL(30, 10) DEFAULT 0,
    realized_pnl DECIMAL(30, 10) DEFAULT 0,
    status VARCHAR(16) NOT NULL DEFAULT 'OPEN',
    updated_at TIMESTAMP NULL,
    KEY idx_run_inst (run_id, inst_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS portfolio_snapshot_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    total_equity DECIMAL(30, 10) NOT NULL,
    available DECIMAL(30, 10) NOT NULL,
    margin DECIMAL(30, 10) NOT NULL,
    pnl DECIMAL(30, 10) NOT NULL,
    ts BIGINT NOT NULL,
    KEY idx_run_ts (run_id, ts)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
