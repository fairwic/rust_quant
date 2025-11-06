pub struct ProfitStopLoss {}

impl ProfitStopLoss {
    pub fn get_fibonacci_level(inst_id: &str, period: &str) -> Vec<f64> {
        let multiplier = match period {
            "5m" | "1H" => {
                if inst_id == "BTC-USDT-SWAP" {
                    1.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    2.0
                } else {
                    5.0
                }
            }
            "4H" => {
                if inst_id == "BTC-USDT-SWAP" {
                    3.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    5.0
                } else {
                    8.0
                }
            }
            "1D" => {
                if inst_id == "BTC-USDT-SWAP" {
                    4.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    8.0
                } else {
                    18.0
                }
            }
            "5D" => {
                if inst_id == "BTC-USDT-SWAP" {
                    10.0
                } else if inst_id == "ETH-USDT-SWAP" {
                    20.0
                } else {
                    40.0
                }
            }
            _ => 1.0, // 默认不改变倍率
        };

        let mut array = vec![0.00186, 0.00382, 0.005, 0.00618, 0.00786, 0.01];
        array.iter_mut().for_each(|x| *x *= multiplier);
        array
    }
}
