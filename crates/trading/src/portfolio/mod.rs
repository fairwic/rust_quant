#[derive(Debug, Clone)]
pub struct FillEvent {
    /// 交易方向。
    pub side: String,
    /// 数量。
    pub qty: f64,
    /// 价格。
    pub price: f64,
}
#[derive(Debug)]
pub struct PortfolioManager {
    /// total权益，用于当前结构体的业务数据。
    total_equity: f64,
}
impl PortfolioManager {
    pub fn new(total_equity: f64) -> Self {
        Self { total_equity }
    }
    /// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    pub fn apply_fill(&mut self, _fill: FillEvent) {
        let delta = _fill.price * _fill.qty;
        if _fill.side.eq_ignore_ascii_case("BUY") {
            self.total_equity -= delta;
        } else if _fill.side.eq_ignore_ascii_case("SELL") {
            self.total_equity += delta;
        }
    }
    pub fn total_equity(&self) -> f64 {
        self.total_equity
    }
}
#[cfg(test)]
mod tests {
    use super::{FillEvent, PortfolioManager};
    #[test]
    /// 提供portfolioupdatesonfill的集中实现，避免量化核心调用方重复处理相同细节。
    fn portfolio_updates_on_fill() {
        let mut pm = PortfolioManager::new(100.0);
        pm.apply_fill(FillEvent {
            side: "BUY".to_string(),
            qty: 1.0,
            price: 10.0,
        });
        assert!(pm.total_equity() < 100.0);
    }
}
