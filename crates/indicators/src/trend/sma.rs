#[derive(Debug)]
pub struct Sma {
    /// peroid，用于交易策略计算。
    peroid: usize,
    /// sum，用于交易策略计算。
    sum: f64,
    /// 列表数据。
    values: Vec<f64>,
}
impl Sma {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(length: usize) -> Self {
        Self {
            peroid: length,
            sum: 0.0,
            values: Vec::with_capacity(length),
        }
    }
    /// 推进指标到下一根 K 线，并返回最新计算结果。
    pub fn next(&mut self, price: f64) -> f64 {
        // 如果窗口未满，添加新的值并累加到 sum
        self.values.push(price);
        if self.values.len() < self.peroid {
            self.sum += price;
        } else {
            self.sum += price; // 更新 sum
            if self.values.len() > self.peroid {
                // 滑动窗口：移除最旧的元素，加入新的元素
                let oldest_value = self.values.remove(0); // 移除最旧的元素
                self.sum -= oldest_value; // 从 sum 中减去最旧的元素
            }
        }
        // 返回当前窗口的 SMA
        self.sum / self.peroid as f64
    }
}
