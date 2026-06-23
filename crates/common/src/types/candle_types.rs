use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CandleItem {
    /// o，用于行情、K 线或市场扫描。
    pub o: f64,
    /// h，用于行情、K 线或市场扫描。
    pub h: f64,
    /// l，用于行情、K 线或市场扫描。
    pub l: f64,
    /// c，用于行情、K 线或市场扫描。
    pub c: f64,
    /// v，用于行情、K 线或市场扫描。
    pub v: f64,
    /// 事件时间戳。
    pub ts: i64,
    /// confirm，用于行情、K 线或市场扫描。
    pub confirm: i32,
}
impl CandleItem {
    pub fn builder() -> CandleItemBuilder {
        CandleItemBuilder::default()
    }
    pub fn ts(&self) -> i64 {
        self.ts
    }
    pub fn o(&self) -> f64 {
        self.o
    }
    pub fn h(&self) -> f64 {
        self.h
    }
    pub fn l(&self) -> f64 {
        self.l
    }
    pub fn c(&self) -> f64 {
        self.c
    }
    pub fn v(&self) -> f64 {
        self.v
    }
    pub fn confirm(&self) -> i32 {
        self.confirm
    }
    /// 提供请求体ratio的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn body_ratio(&self) -> f64 {
        let body = (self.c - self.o).abs();
        let range = self.h - self.l;
        body / range
    }
    /// 提供upshadowratio的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn up_shadow_ratio(&self) -> f64 {
        if self.c < self.o {
            //下跌
            (self.h - self.o) / (self.h - self.l)
        } else {
            //上涨
            (self.h - self.c) / (self.h - self.l)
        }
    }
    /// 提供向下取整shadowratio的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn down_shadow_ratio(&self) -> f64 {
        if self.c < self.o {
            //下跌
            (self.c - self.l) / (self.h - self.l)
        } else {
            //上涨
            (self.o - self.l) / (self.h - self.l)
        }
    }
    //如果上影线,和下影线 都占比总高度超过30%,则说明k线实体部分占比非常小(20%)
    pub fn is_small_body_and_big_up_down_shadow(&self) -> bool {
        // if self.ts == 1760860800000 {
        //     println!("up_shadow_ratio: {:?}", self.up_shadow_ratio());
        //     println!("down_shadow_ratio: {:?}", self.down_shadow_ratio());
        // }
        self.up_shadow_ratio() > 0.3 && self.down_shadow_ratio() > 0.3
    }
}
#[derive(Default)]
pub struct CandleItemBuilder {
    /// o；为空时表示该条件不启用。
    o: Option<f64>,
    /// h；为空时表示该条件不启用。
    h: Option<f64>,
    /// l；为空时表示该条件不启用。
    l: Option<f64>,
    /// c；为空时表示该条件不启用。
    c: Option<f64>,
    /// v；为空时表示该条件不启用。
    v: Option<f64>,
    /// 事件时间戳。
    ts: Option<i64>,
    /// 确认标记；为空时表示未确认。
    confirm: Option<i32>,
}
impl CandleItemBuilder {
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    pub fn ts(mut self, val: i64) -> Self {
        self.ts = Some(val);
        self
    }
    /// 提供o的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn o(mut self, val: f64) -> Self {
        self.o = Some(val);
        self
    }
    /// 提供h的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn h(mut self, val: f64) -> Self {
        self.h = Some(val);
        self
    }
    /// 提供l的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn l(mut self, val: f64) -> Self {
        self.l = Some(val);
        self
    }
    /// 提供c的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn c(mut self, val: f64) -> Self {
        self.c = Some(val);
        self
    }
    /// 提供v的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn v(mut self, val: f64) -> Self {
        self.v = Some(val);
        self
    }
    /// 提供confirm的集中实现，避免行情数据调用方重复处理相同细节。
    pub fn confirm(mut self, val: i32) -> Self {
        self.confirm = Some(val);
        self
    }
    /// 构建build，集中维护行情数据的字段组装规则。
    pub fn build(self) -> anyhow::Result<CandleItem> {
        if let (Some(o), Some(h), Some(l), Some(c), Some(v), Some(ts)) =
            (self.o, self.h, self.l, self.c, self.v, self.ts)
        {
            if l <= o && l <= c && l <= h && h >= o && h >= c && v >= 0.0 && l >= 0.0 {
                Ok(CandleItem {
                    o,
                    h,
                    l,
                    c,
                    v,
                    ts,
                    confirm: self.confirm.unwrap_or(1),
                })
            } else {
                Err(anyhow::anyhow!("CandleItemInvalid"))
            }
        } else {
            Err(anyhow::anyhow!("CandleItemIncomplete"))
        }
    }
}
