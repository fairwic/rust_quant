-- Add migration script here
CREATE TABLE market_anomalies (
    id BIGINT AUTO_INCREMENT PRIMARY KEY COMMENT '主键ID',
    symbol VARCHAR(32) NOT NULL COMMENT '交易对',
    vol_delta DECIMAL(30, 10) NOT NULL COMMENT '24小时成交量变化量',
    price_change_percent DECIMAL(10, 4) COMMENT '价格变化百分比',
    detected_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '检测时间',
    status VARCHAR(16) NOT NULL DEFAULT 'OPEN' COMMENT '状态: OPEN, PROMOTED, IGNORED',
    
    INDEX idx_symbol_ts (symbol, detected_at),
    INDEX idx_detected_at (detected_at)
) COMMENT '市场异动记录表';

CREATE TABLE fund_flow_alerts (
    id BIGINT AUTO_INCREMENT PRIMARY KEY COMMENT '主键ID',
    symbol VARCHAR(32) NOT NULL COMMENT '交易对',
    net_inflow DECIMAL(30, 10) NOT NULL COMMENT '净流入金额',
    total_volume DECIMAL(30, 10) NOT NULL COMMENT '总成交金额(窗口内)',
    side VARCHAR(16) NOT NULL COMMENT '方向: INFLOW, OUTFLOW',
    window_secs INT NOT NULL DEFAULT 60 COMMENT '统计窗口秒数',
    alert_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '报警时间',
    
    INDEX idx_symbol_ts (symbol, alert_at),
    INDEX idx_alert_at (alert_at)
) COMMENT '资金流向报警表';
