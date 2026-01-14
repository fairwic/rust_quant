use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize, Clone)]
pub struct SimulationResult {
    pub max_drawdown: f64,
    pub total_profit: f64,
    pub final_capital: f64,
    pub win_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct MonteCarloReport {
    pub iterations: usize,
    pub original_pnl: Vec<f64>,
    pub max_drawdown_stats: Stats,
    pub profit_stats: Stats,
    pub simulations: Vec<SimulationResult>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub p95: f64, // 95% worse case (for DD) or best case
    pub p50: f64, // Median
    pub p05: f64, // 5% best case (for DD) or worse case
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

pub struct MonteCarloAnalyzer {
    initial_capital: f64,
}

impl MonteCarloAnalyzer {
    pub fn new(initial_capital: f64) -> Self {
        Self { initial_capital }
    }

    pub fn simulate(&self, pnls: &[f64], iterations: usize) -> MonteCarloReport {
        let mut rng = thread_rng();
        let mut simulations = Vec::with_capacity(iterations);

        let mut shuffled_pnls = pnls.to_vec();

        for _ in 0..iterations {
            shuffled_pnls.shuffle(&mut rng);
            let result = self.calculate_metrics(&shuffled_pnls);
            simulations.push(result);
        }

        let max_drawdowns: Vec<f64> = simulations.iter().map(|s| s.max_drawdown).collect();
        let profits: Vec<f64> = simulations.iter().map(|s| s.total_profit).collect();

        MonteCarloReport {
            iterations,
            original_pnl: pnls.to_vec(),
            max_drawdown_stats: Self::calculate_stats(&max_drawdowns),
            profit_stats: Self::calculate_stats(&profits),
            simulations,
        }
    }

    fn calculate_metrics(&self, pnls: &[f64]) -> SimulationResult {
        let mut current_capital = self.initial_capital;
        let mut peak_capital = self.initial_capital;
        let mut max_drawdown = 0.0;
        let mut total_profit = 0.0;
        let mut wins = 0;

        for &pnl in pnls {
            // For losing trades, estimate intra-trade floating loss as 1.5x actual loss
            // This matches the backtest's conservative drawdown estimation
            if pnl < 0.0 {
                let estimated_worst = current_capital + pnl * 1.5;
                let worst_capital = estimated_worst.max(0.0);

                // Check drawdown at estimated worst point
                if worst_capital < peak_capital && peak_capital > 0.0 {
                    let dd = (peak_capital - worst_capital) / peak_capital;
                    if dd > max_drawdown {
                        max_drawdown = dd;
                    }
                }
            }

            // Apply actual PnL
            current_capital += pnl;
            total_profit += pnl;

            if pnl > 0.0 {
                wins += 1;
            }

            // Update peak after trade closes
            if current_capital > peak_capital {
                peak_capital = current_capital;
            }

            // Check drawdown at trade close
            if peak_capital > 0.0 {
                let dd = (peak_capital - current_capital) / peak_capital;
                if dd > max_drawdown {
                    max_drawdown = dd;
                }
            }
        }

        SimulationResult {
            max_drawdown,
            total_profit,
            final_capital: current_capital,
            win_rate: if !pnls.is_empty() {
                wins as f64 / pnls.len() as f64
            } else {
                0.0
            },
        }
    }

    fn calculate_stats(values: &[f64]) -> Stats {
        let mut sorted = values.to_vec();
        // handling potential NaNs by partial_cmp or unwrap_or
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let len = sorted.len();
        if len == 0 {
            return Stats {
                p95: 0.0,
                p50: 0.0,
                p05: 0.0,
                mean: 0.0,
                min: 0.0,
                max: 0.0,
            };
        }

        let sum: f64 = sorted.iter().sum();

        Stats {
            p95: sorted[(len as f64 * 0.95) as usize],
            p50: sorted[len / 2],
            p05: sorted[(len as f64 * 0.05) as usize],
            mean: sum / len as f64,
            min: sorted[0],
            max: sorted[len - 1],
        }
    }
}

impl fmt::Display for MonteCarloReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Monte Carlo Simulation Report ({} iterations)",
            self.iterations
        )?;
        writeln!(f, "--------------------------------------------------")?;
        writeln!(f, "Max Drawdown Statistics:")?;
        writeln!(
            f,
            "  P95 (Worst 5%): {:.2}%",
            self.max_drawdown_stats.p95 * 100.0
        )?;
        writeln!(
            f,
            "  Mean:           {:.2}%",
            self.max_drawdown_stats.mean * 100.0
        )?;
        writeln!(
            f,
            "  P50 (Median):   {:.2}%",
            self.max_drawdown_stats.p50 * 100.0
        )?;
        writeln!(
            f,
            "  P05 (Best 5%):  {:.2}%",
            self.max_drawdown_stats.p05 * 100.0
        )?;
        writeln!(f, "--------------------------------------------------")?;
        writeln!(f, "Profit Statistics:")?;
        writeln!(f, "  P05 (Worst 5%): {:.2}", self.profit_stats.p05)?;
        writeln!(f, "  Mean:           {:.2}", self.profit_stats.mean)?;
        writeln!(f, "  P50 (Median):   {:.2}", self.profit_stats.p50)?;
        writeln!(f, "  P95 (Best 5%):  {:.2}", self.profit_stats.p95)?;
        Ok(())
    }
}
