-- Add rank columns to market_anomalies
ALTER TABLE market_anomalies
ADD COLUMN current_rank INT COMMENT '当前成交额排名',
ADD COLUMN previous_rank INT COMMENT '上次成交额排名',
ADD COLUMN rank_delta INT COMMENT '排名变化量 (positive means moved up)';
