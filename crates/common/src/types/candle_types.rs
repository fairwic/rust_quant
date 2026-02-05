use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CandleItem {
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub c: f64,
    pub v: f64,
    pub ts: i64,
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

    pub fn body_ratio(&self) -> f64 {
        let body = (self.c - self.o).abs();
        let range = self.h - self.l;
        body / range
    }
    pub fn up_shadow_ratio(&self) -> f64 {
        if self.c < self.o {
            //下跌
            (self.h - self.o) / (self.h - self.l)
        } else {
            //上涨
            (self.h - self.c) / (self.h - self.l)
        }
    }
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
    o: Option<f64>,
    h: Option<f64>,
    l: Option<f64>,
    c: Option<f64>,
    v: Option<f64>,
    ts: Option<i64>,
    confirm: Option<i32>,
}

impl CandleItemBuilder {
    pub fn ts(mut self, val: i64) -> Self {
        self.ts = Some(val);
        self
    }
    pub fn o(mut self, val: f64) -> Self {
        self.o = Some(val);
        self
    }
    pub fn h(mut self, val: f64) -> Self {
        self.h = Some(val);
        self
    }
    pub fn l(mut self, val: f64) -> Self {
        self.l = Some(val);
        self
    }
    pub fn c(mut self, val: f64) -> Self {
        self.c = Some(val);
        self
    }
    pub fn v(mut self, val: f64) -> Self {
        self.v = Some(val);
        self
    }
    pub fn confirm(mut self, val: i32) -> Self {
        self.confirm = Some(val);
        self
    }

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
