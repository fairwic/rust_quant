-- 添加止损更新历史字段
-- 用于记录所有止损价格更新的完整历史(JSON格式)

ALTER TABLE back_test_detail 
ADD COLUMN stop_loss_update_history TEXT COMMENT '止损更新历史(JSON格式,存储Vec<StopLossUpdate>)';
