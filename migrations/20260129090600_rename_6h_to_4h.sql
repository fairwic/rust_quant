-- 将 6h 周期改为 4h 周期
ALTER TABLE market_anomalies 
    CHANGE COLUMN rank_6h_ago rank_4h_ago INT,
    CHANGE COLUMN delta_6h delta_4h INT;
