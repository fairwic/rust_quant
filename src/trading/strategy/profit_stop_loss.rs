pub struct ProfitStopLoss {}

impl ProfitStopLoss {
    pub fn get_fibonacci_level(inst_id: &str, period: &str) -> Vec<f64> {
        let multiplier = match period {
            "5m" | "1H" => {
                if inst_id == "BTC-USDT_SWAP" {
                    2.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    4.0
                } else {
                    8.0
                }
            }
            "4H" => {
                if inst_id == "BTC-USDT_SWAP" {
                    3.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    9.0
                } else {
                    12.0
                }
            }
            "1D" => {
                if inst_id == "BTC-USDT_SWAP" {
                    5.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    10.0
                } else {
                    20.0
                }
            }
            "5D" => {
                if inst_id == "BTC-USDT_SWAP" {
                    10.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    20.0
                } else {
                    40.0
                }
            }
            _ => 1.0, // 默认不改变倍率
        };

        let mut array = vec![0.00236, 0.00382, 0.005, 0.00618, 0.00786, 0.01];
        array.iter_mut().for_each(|x| *x *= multiplier);
        array
    }
}