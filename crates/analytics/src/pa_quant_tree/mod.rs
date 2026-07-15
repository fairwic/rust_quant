//! PA Quant Tree 的离线研究、统计验证和 Champion/Challenger 生命周期。
//!
//! 本模块不参与实时信号、订单或 AI 调用；所有输入必须是时间点一致的已结算研究记录。

mod dataset;
mod evidence;
mod experiment_ledger;
mod historical;
mod lifecycle;
mod metrics;
mod paired_counterfactual;
mod portfolio;
mod readiness;
mod trainer;
mod validation;

pub use dataset::*;
pub use evidence::*;
pub use experiment_ledger::*;
pub use historical::*;
pub use lifecycle::*;
pub use metrics::*;
pub use paired_counterfactual::*;
pub use portfolio::*;
pub use readiness::*;
pub use trainer::*;
pub use validation::*;
