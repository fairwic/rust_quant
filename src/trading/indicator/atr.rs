use ta::indicators::SimpleMovingAverage;
use ta::Next;

pub struct ATR {
    period: usize,
    sma:SimpleMovingAverage,
    prev_close: Option<f64>,    // 上一个周期的close值
    tr_buffer: Vec<f64>,        // 用于缓存TrueRange值
    sum: f64,                   // 平滑后的ATR值
    alpha: f64,                 // 平滑因子，1/period
}

impl ATR {
    pub fn new(period: usize) -> Self {
        ATR {
            period,
            sma:SimpleMovingAverage::new(period).unwrap(),
            prev_close: None,
            tr_buffer: Vec::with_capacity(period),
            sum: 0.0,
            alpha: 1.0 / period as f64,
        }
    }

    pub fn next(&mut self, high: f64, low: f64, close: f64) -> f64 {
        // 计算当前周期的TrueRange
        let tr = self.true_range(high, low, close);

        // println!("truerange:{:?}",tr);
        // 如果缓存未满，填充缓存
        if self.tr_buffer.len() < self.period {
            self.tr_buffer.push(tr);
            self.prev_close = Some(close);
        }

        // 滑动窗口，移除最旧的TR值
        if self.tr_buffer.len() == self.period {
            self.tr_buffer.remove(0);
        }

        // 将当前TR添加到缓存中
        self.tr_buffer.push(tr);

        if self.tr_buffer.len()==0 {
            self.sum=self.sma.next(tr)
        }else {
            // 计算ATR的平滑值（递归计算）
            self.sum = self.alpha * tr + (1.0 - self.alpha) * self.sum;
        }
        // println!("sum:{:?}",self.sum);
        // 返回平滑后的ATR
        self.prev_close = Some(close);
        self.sum
    }

    //437.85+

    // 计算TrueRange
    fn true_range(&self, high: f64, low: f64, close: f64) -> f64 {
        match self.prev_close {
            None => high - low, // 如果是第一个周期，返回高低差
            Some(prev_close) => {
                let tr1 = high - low;
                let tr2 = (high - prev_close).abs();
                let tr3 = (low - prev_close).abs();
                tr1.max(tr2).max(tr3) // 计算TrueRange的最大值
            }
        }
    }
}
