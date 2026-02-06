#[derive(Debug, Clone)]
pub struct FillEvent {
    pub side: String,
    pub qty: f64,
    pub price: f64,
}

#[derive(Debug)]
pub struct PortfolioManager {
    total_equity: f64,
}

impl PortfolioManager {
    pub fn new(total_equity: f64) -> Self {
        Self { total_equity }
    }

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
