#[derive(Debug)]
pub struct Sma {
    peroid: usize,
    sum: f64,
    values: Vec<f64>,
}

impl Sma {
    pub fn new(length: usize) -> Self {
        Self {
            peroid: length,
            sum: 0.0,
            values: Vec::with_capacity(length),
        }
    }

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
