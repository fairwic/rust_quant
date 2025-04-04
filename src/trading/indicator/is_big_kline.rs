use crate::CandleItem;

//判断当前k线是否是大阳线或者大阴线,实体部分占比大于80%
pub struct IsBigKLineIndicator {
    pub amplitude: f64,
    pub is_big_k_line: bool,
}
impl Default for IsBigKLineIndicator {
    fn default() -> Self {
        Self {
            amplitude: 70.0,
            is_big_k_line: false,
        }
    }
}

impl IsBigKLineIndicator {
    pub fn new(amplitude: f64) -> Self {
        Self {
            amplitude,
            is_big_k_line: false,
        }
    }
    //判断当前k线是否是大阳线或者大阴线
    pub fn is_big_k_line(&self, data_item: &CandleItem) -> bool {
        let open = data_item.o();
        let close = data_item.c();
        let high = data_item.h();
        let low = data_item.l();
        println!("data_item: {:?}", data_item);

        //计算实体部分占比
        let amplitude = if open < close {
            //上涨
            //收盘-开盘/最高-开盘 ，如果high==open，则认为是十字星
            if high == open {
                return false;
            }
            (close - open) / (high - open) * 100.0
        } else {
            //下跌
            //开盘-收盘/开盘-最低
            if low == open {
                return false;
            }
            (open - close) / (open - low) * 100.0
        };
        println!("amplitude: {:?}", amplitude);
        amplitude > self.amplitude
    }
    pub fn amplitude(&self) -> f64 {
        self.amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_big_k_line() {
        let data_item = CandleItem::builder()
            .o(100.0)
            .c(120.0)
            .h(120.0)
            .l(90.0)
            .v(100.0)
            .ts(1741514400000)
            .build()
            .unwrap();
        let is_big_k_line = IsBigKLineIndicator::new(80.0).is_big_k_line(&data_item);
        assert_eq!(is_big_k_line, true);
    }

    #[test]
    fn test_is_big_k_line_2() {
        let data_item = CandleItem::builder()
            .o(100.0)
            .c(90.0)
            .h(110.0)
            .l(89.0)
            .v(100.0)
            .ts(1741514400000)
            .build()
            .unwrap();
        let is_big_k_line = IsBigKLineIndicator::new(80.0).is_big_k_line(&data_item);
        assert_eq!(is_big_k_line, true);
    }
}
