use crate::trading::indicator::sma::Sma;

#[derive(Debug)]
pub struct Rma {
    peroid: usize,
    alpha: f64,            // 平滑因子
    prev_rma: Option<f64>, // 上一周期的 RMA 值
    values: Vec<f64>,      // 用于计算 SMA
    sma: Sma,
    sum: f64,
}

impl Rma {
    // 构造器，初始化 RMA 的周期、平滑因子以及之前的 RMA 和价格值
    pub fn new(length: usize) -> Self {
        Self {
            peroid: length,
            alpha: 1.0 / length as f64,
            prev_rma: None,                     // 初始时没有前一个 RMA
            values: Vec::with_capacity(length), // 用于计算 SMA
            sma: Sma::new(length),
            sum: 0.00,
        }
    }

    // 计算下一个 RMA 值
    pub fn next(&mut self, price: f64) -> f64 {
        // 第一次计算时，返回 SMA
        self.values.push(price);
        if self.values.len() == 0 {
            let sma = self.sma.next(price);
            self.sum += sma;
            return sma;
        }
        if self.values.len() > self.peroid {
            // 滑动窗口：移除最旧的元素，加入新的元素
            let oldest_value = self.values.remove(0); // 移除最旧的元素
            self.sum -= oldest_value; // 从 sum 中减去最旧的元素
        }
        // 非第一次计算，使用递归公式
        self.sum = self.sum + self.alpha * price + (1.0 - self.alpha) * self.sum;

        self.sum
    }
}
