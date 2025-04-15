#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(unused_assignments)]
#![allow(unused_must_use)]

use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio_cron_scheduler::JobScheduler;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

// 定义全局调度器容器，会在需要时被初始化
pub static SCHEDULER: Lazy<Mutex<Option<Arc<JobScheduler>>>> = Lazy::new(|| {
    Mutex::new(None)
});

// 初始化调度器的辅助函数
pub async fn init_scheduler() -> anyhow::Result<Arc<JobScheduler>> {
    let mut lock = SCHEDULER.lock().await;
    
    if lock.is_none() {
        // 只有在调度器未初始化时才创建
        let scheduler = JobScheduler::new().await?;
        let arc_scheduler = Arc::new(scheduler);
        *lock = Some(Arc::clone(&arc_scheduler));
        return Ok(arc_scheduler);
    }
    
    // 返回已存在的调度器
    Ok(Arc::clone(lock.as_ref().unwrap()))
}

pub mod app_config;
pub mod job;
pub mod socket;
pub mod time_util;
pub mod trading;

#[derive(Debug, Clone,Deserialize,Serialize)]
pub struct CandleItem {
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    v: f64,
    ts: i64,
}

impl CandleItem {
    pub fn builder() -> CandleItemBuilder {
        CandleItemBuilder::new()
    }
    fn ts(&self) -> i64 {
        self.ts
    }

    fn o(&self) -> f64 {
        self.o
    }

    fn h(&self) -> f64 {
        self.h
    }

    fn l(&self) -> f64 {
        self.l
    }

    fn c(&self) -> f64 {
        self.c
    }

    fn v(&self) -> f64 {
        self.v
    }
}

pub struct CandleItemBuilder {
    o: Option<f64>,
    h: Option<f64>,
    l: Option<f64>,
    c: Option<f64>,
    v: Option<f64>,
    ts: Option<i64>,
}

impl CandleItemBuilder {
    pub fn new() -> Self {
        Self {
            o: None,
            h: None,
            l: None,
            c: None,
            v: None,
            ts: None,
        }
    }
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

    pub fn build(self) -> anyhow::Result<CandleItem> {
        if let (Some(o), Some(h), Some(l), Some(c), Some(v), Some(ts)) =
            (self.o, self.h, self.l, self.c, self.v, self.ts)
        {
            // validate
            if l <= o && l <= c && l <= h && h >= o && h >= c && v >= 0.0 && l >= 0.0 {
                let item = CandleItem { o, h, l, c, v, ts };
                Ok(item)
            } else {
                Err(anyhow::anyhow!("CandleItemInvalid"))
            }
        } else {
            Err(anyhow::anyhow!("CandleItemIncomplete"))
        }
    }
}
