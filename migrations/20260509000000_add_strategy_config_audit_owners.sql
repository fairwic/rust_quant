ALTER TABLE strategy_configs
    ADD COLUMN IF NOT EXISTS created_by TEXT,
    ADD COLUMN IF NOT EXISTS updated_by TEXT;

COMMENT ON COLUMN strategy_configs.created_by IS '创建者用户名';
COMMENT ON COLUMN strategy_configs.updated_by IS '最后一次编辑者用户名';
