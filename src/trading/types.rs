use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CandleItem {
    pub(crate) o: f64,
    pub(crate) h: f64,
    pub(crate) l: f64,
    pub(crate) c: f64,
    pub(crate) v: f64,
    pub(crate) ts: i64,
    pub(crate) confirm: i32,
}

impl CandleItem {
    pub fn builder() -> CandleItemBuilder {
        CandleItemBuilder::new()
    }
    pub fn ts(&self) -> i64 { self.ts }
    pub fn o(&self) -> f64 { self.o }
    pub fn h(&self) -> f64 { self.h }
    pub fn l(&self) -> f64 { self.l }
    pub fn c(&self) -> f64 { self.c }
    pub fn v(&self) -> f64 { self.v }
    pub fn confirm(&self) -> i32 { self.confirm }
}

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
    pub fn new() -> Self {
        Self { o: None, h: None, l: None, c: None, v: None, ts: None, confirm: None }
    }
    pub fn ts(mut self, val: i64) -> Self { self.ts = Some(val); self }
    pub fn o(mut self, val: f64) -> Self { self.o = Some(val); self }
    pub fn h(mut self, val: f64) -> Self { self.h = Some(val); self }
    pub fn l(mut self, val: f64) -> Self { self.l = Some(val); self }
    pub fn c(mut self, val: f64) -> Self { self.c = Some(val); self }
    pub fn v(mut self, val: f64) -> Self { self.v = Some(val); self }
    pub fn confirm(mut self, val: i32) -> Self { self.confirm = Some(val); self }

    pub fn build(self) -> anyhow::Result<CandleItem> {
        if let (Some(o), Some(h), Some(l), Some(c), Some(v), Some(ts)) = (self.o, self.h, self.l, self.c, self.v, self.ts) {
            if l <= o && l <= c && l <= h && h >= o && h >= c && v >= 0.0 && l >= 0.0 {
                Ok(CandleItem { o, h, l, c, v, ts, confirm: self.confirm.unwrap_or(1) })
            } else {
                Err(anyhow::anyhow!("CandleItemInvalid"))
            }
        } else {
            Err(anyhow::anyhow!("CandleItemIncomplete"))
        }
    }
}

