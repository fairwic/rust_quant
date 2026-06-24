ALTER TABLE strategy_configs
    ADD COLUMN IF NOT EXISTS risk_level TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS detail TEXT,
    ADD COLUMN IF NOT EXISTS cover_image TEXT,
    ADD COLUMN IF NOT EXISTS display_total_return_pct NUMERIC(12,4),
    ADD COLUMN IF NOT EXISTS display_sharpe_ratio NUMERIC(12,4),
    ADD COLUMN IF NOT EXISTS display_trade_count INT,
    ADD COLUMN IF NOT EXISTS display_max_drawdown_pct NUMERIC(12,4);

COMMENT ON COLUMN strategy_configs.risk_level IS '策略商品默认展示风险等级，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.description IS '策略商品默认简介，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.detail IS '策略商品默认详情，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.cover_image IS '策略商品默认展示图路径，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_total_return_pct IS '策略商品默认展示总收益率百分比，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_sharpe_ratio IS '策略商品默认展示夏普比率，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_trade_count IS '策略商品默认展示累计交易笔数，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_max_drawdown_pct IS '策略商品默认展示最大回撤百分比，可由 Admin 商品配置覆盖';
