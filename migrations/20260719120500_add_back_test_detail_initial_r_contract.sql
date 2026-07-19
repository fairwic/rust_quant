BEGIN;

-- 回测明细必须持久化入场时冻结的保护价与 R，避免移动止损覆盖初始风险口径。
ALTER TABLE IF EXISTS back_test_detail
    ADD COLUMN IF NOT EXISTS initial_stop_price DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS initial_risk_amount DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS net_profit_r DOUBLE PRECISION;

COMMENT ON COLUMN back_test_detail.initial_stop_price IS '入场时冻结的有效保护价，后续移动止损不得覆盖';
COMMENT ON COLUMN back_test_detail.initial_risk_amount IS '本条记录对应数量在初始保护价处的价格风险金额';
COMMENT ON COLUMN back_test_detail.net_profit_r IS '扣除回测手续费后的出场收益除以初始风险金额';

COMMIT;
