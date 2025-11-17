use anyhow::Result;
use okx::dto::market_dto::TickerOkxResDto;
use tracing::debug;

use crate::models::TicketsModel;

/// 市场 Ticker 数据服务
pub struct TickerService {
    model: TicketsModel,
}

impl TickerService {
    pub fn new() -> Self {
        Self {
            model: TicketsModel::new(),
        }
    }

    /// 更新指定交易对的 ticker 数据（存在则更新，不存在则插入）
    pub async fn upsert_tickers(
        &self,
        tickers: Vec<TickerOkxResDto>,
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
                self.model.add(vec![ticker]).await?;
            } else {
                debug!("更新已存在的ticker记录: {}", inst_id);
                self.model.update(&ticker).await?;
            }
        }

        Ok(())
    }
}
