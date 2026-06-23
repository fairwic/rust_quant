use crate::models::{TickersDataEntity, TicketsModel};
use anyhow::Result;
use tracing::debug;
/// 市场 Ticker 数据服务
pub struct TickerService {
    /// 模型。
    model: TicketsModel,
}
impl TickerService {
    /// 构建 行情与市场数据 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Self {
        Self {
            model: TicketsModel::new(),
        }
    }
    /// 更新指定交易对的 ticker 数据（存在则更新，不存在则插入）
    pub async fn upsert_tickers(
        &self,
        tickers: Vec<TickersDataEntity>,
        filter_inst_ids: &[String],
    ) -> Result<()> {
        if tickers.is_empty() {
            return Ok(());
        }
        for ticker in tickers.into_iter() {
            let inst_id = ticker.inst_id.clone();
            if !filter_inst_ids.is_empty() && !filter_inst_ids.contains(&inst_id) {
                debug!("跳过未订阅的ticker: {}", inst_id);
                continue;
            }
            let existing = self.model.find_one(&inst_id).await?;
            if existing.is_empty() {
                debug!("插入新的ticker记录: {}", inst_id);
                self.model.add_entities(vec![ticker]).await?;
            } else {
                debug!("更新已存在的ticker记录: {}", inst_id);
                self.model.update_entity(&ticker).await?;
            }
        }
        Ok(())
    }
}
impl Default for TickerService {
    fn default() -> Self {
        Self::new()
    }
}
