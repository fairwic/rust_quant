//! 交易所适配器模块
//!
//! 实现各个交易所的adapter，统一接口

mod factory;
mod okx_adapter;

pub use factory::ExchangeFactory;
pub use okx_adapter::{
    OkxAccountAdapter, OkxContractsAdapter, OkxMarketDataAdapter, OkxPublicDataAdapter,
};

// 未来添加其他交易所：
// mod binance_adapter;
// mod bybit_adapter;
// pub use binance_adapter::*;
// pub use bybit_adapter::*;
