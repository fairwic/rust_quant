use rust_quant_common::CandleItem;
const MIN_PREVIOUS_BODY_RATIO: f64 = 0.001;
const MIN_PREVIOUS_BODY_ABS: f64 = 1.0;

/// 判断前一根 K 线实体是否足够承载反转形态；比例适配低价币，绝对值兜底高价币。
fn has_meaningful_previous_body(kline: &CandleItem) -> bool {
    let body = (kline.c - kline.o).abs();
    let body_ratio = body / kline.o.abs().max(1e-9);
    body_ratio >= MIN_PREVIOUS_BODY_RATIO || body >= MIN_PREVIOUS_BODY_ABS
}

/// 判断多头实体吞没：当前阳线实体必须完整覆盖前一根有意义的阴线实体。
fn is_bullish_body_engulfing(current: &CandleItem, previous: &CandleItem) -> bool {
    previous.c < previous.o
        && has_meaningful_previous_body(previous)
        && current.c > current.o
        && current.o <= previous.c
        && current.c >= previous.o
}

/// 判断空头实体吞没：当前阴线实体必须完整覆盖前一根有意义的阳线实体。
fn is_bearish_body_engulfing(current: &CandleItem, previous: &CandleItem) -> bool {
    previous.c > previous.o
        && has_meaningful_previous_body(previous)
        && current.c < current.o
        && current.o >= previous.c
        && current.c <= previous.o
}
/// 成交量比率指标
/// 计算当前成交量与历史n根K线的平均值的比值
#[derive(Debug, Clone)]
pub struct KlineEngulfingIndicator {
    // 吞没形态指标值
    last_kline: Option<CandleItem>,
    // 前前一根K线（用于过滤）
    prev_prev_kline: Option<CandleItem>,
}
#[derive(Debug, Clone)]
pub struct KlineEngulfingOutput {
    /// 是否为吞没形态。
    pub is_engulfing: bool,
    /// body 比例。
    pub body_ratio: f64,
}
impl KlineEngulfingIndicator {
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Self {
        Self {
            last_kline: None,
            prev_prev_kline: None,
        }
    }
    /// 推进指标到下一根 K 线，并返回最新计算结果。
    pub fn next(&mut self, current_kline: &CandleItem) -> KlineEngulfingOutput {
        if self.last_kline.is_none() {
            self.last_kline = Some(current_kline.clone());
            return KlineEngulfingOutput {
                is_engulfing: false,
                body_ratio: 0.0,
            };
        }
        let last_kline = self.last_kline.as_ref().unwrap();
        // 吞没只认可实体吞没实体；前一根实体太小视为噪声，不作为反转形态基础。
        let mut is_bullish = is_bullish_body_engulfing(current_kline, last_kline);
        // 【新增过滤逻辑】
        // 如果前前根K线是大阴线（实体>2.0%），且当前反转没有吞没它，则视为无效
        if is_bullish {
            if let Some(prev_a) = &self.prev_prev_kline {
                let is_bear_a = prev_a.c < prev_a.o;
                if is_bear_a {
                    let a_body_pct = (prev_a.o - prev_a.c).abs() / prev_a.o;
                    // 此处阈值定为 2.0%
                    if a_body_pct > 0.02 {
                        // 如果当前收盘价没能超过A的开盘价，则视为多头力量不足以扭转大跌趋势
                        if current_kline.c < prev_a.o {
                            is_bullish = false;
                        }
                    }
                }
            }
        }
        let mut is_bearish = is_bearish_body_engulfing(current_kline, last_kline);
        // 【新增过滤逻辑】
        // 如果前前根K线是大阳线（实体>2.0%），且当前反转没有吞没它，则视为无效
        if is_bearish {
            if let Some(prev_a) = &self.prev_prev_kline {
                let is_bull_a = prev_a.c > prev_a.o;
                if is_bull_a {
                    let a_body_pct = (prev_a.c - prev_a.o).abs() / prev_a.o;
                    if a_body_pct > 0.02 {
                        // 如果当前收盘价没能低于A的开盘价，则视为空头力量不足
                        if current_kline.c > prev_a.o {
                            is_bearish = false;
                        }
                    }
                }
            }
        }
        let body_ratio = if is_bullish || is_bearish {
            //计算实体比例,当前k线实体部分与当前k线路的上下影线部分的比例
            let body_size = (current_kline.c - current_kline.o).abs();
            let shadow_size = current_kline.h - current_kline.l;
            if shadow_size == 0.0 {
                0.0
            } else {
                body_size / shadow_size
            }
        } else {
            0.0
        };
        // 更新历史
        self.prev_prev_kline = self.last_kline.clone();
        self.last_kline = Some(current_kline.clone());
        KlineEngulfingOutput {
            is_engulfing: is_bullish || is_bearish,
            body_ratio,
        }
    }
}
impl Default for KlineEngulfingIndicator {
    fn default() -> Self {
        Self::new()
    }
}
//添加测试单例
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn test_engulfing_indicator() {
        let mut indicator = KlineEngulfingIndicator::new();
        // 创建一个看涨吞没的例子
        let kline1 = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 95.0,
            ts: 0,
            v: 0.00,
            confirm: 0,
        };
        let kline2 = CandleItem {
            o: 90.0,
            h: 116.0,
            l: 85.0,
            c: 112.0,
            ts: 1,
            v: 0.00,
            confirm: 0,
        };
        indicator.next(&kline1);
        let output = indicator.next(&kline2);
        assert!(output.is_engulfing);
        assert!(output.body_ratio > 0.0);
        // 创建一个看跌吞没的例子
        let kline3 = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 105.0,
            ts: 2,
            v: 0.00,
            confirm: 0,
        };
        let kline4 = CandleItem {
            o: 112.0,
            h: 115.0,
            l: 85.0,
            c: 88.0,
            ts: 3,
            v: 0.00,
            confirm: 0,
        };
        indicator.next(&kline3);
        let output = indicator.next(&kline4);
        println!("{:?}", output);
        assert!(output.is_engulfing);
        assert!(output.body_ratio > 0.0);
        // 创建一个非吞没的例子
        let kline5 = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 105.0,
            ts: 4,
            v: 0.00,
            confirm: 0,
        };
        let kline6 = CandleItem {
            o: 105.0,
            h: 115.0,
            l: 95.0,
            c: 110.0,
            ts: 5,
            v: 0.00,
            confirm: 0,
        };
        indicator.next(&kline5);
        let output = indicator.next(&kline6);
        assert!(!output.is_engulfing);
        assert_eq!(output.body_ratio, 0.0);
    }

    #[test]
    fn tiny_previous_body_does_not_create_bullish_engulfing_signal() {
        let mut indicator = KlineEngulfingIndicator::new();
        let previous = CandleItem {
            o: 1757.48,
            h: 1764.74,
            l: 1754.0,
            c: 1757.27,
            ts: 1783152000000,
            v: 403529760.17518,
            confirm: 1,
        };
        let current = CandleItem {
            o: 1757.28,
            h: 1803.33,
            l: 1757.28,
            c: 1790.57,
            ts: 1783166400000,
            v: 1774457283.17251,
            confirm: 1,
        };

        indicator.next(&previous);
        let output = indicator.next(&current);

        assert!(!output.is_engulfing);
        assert_eq!(output.body_ratio, 0.0);
    }

    #[test]
    fn bullish_engulfing_requires_current_body_to_cover_previous_body() {
        let mut indicator = KlineEngulfingIndicator::new();
        let previous = CandleItem {
            o: 100.0,
            h: 104.0,
            l: 96.0,
            c: 98.0,
            ts: 10,
            v: 1.0,
            confirm: 1,
        };
        let current_closes_over_high_without_body_engulfing = CandleItem {
            o: 98.5,
            h: 105.0,
            l: 98.0,
            c: 104.5,
            ts: 11,
            v: 1.0,
            confirm: 1,
        };

        indicator.next(&previous);
        let output = indicator.next(&current_closes_over_high_without_body_engulfing);

        assert!(!output.is_engulfing);
        assert_eq!(output.body_ratio, 0.0);
    }
}
