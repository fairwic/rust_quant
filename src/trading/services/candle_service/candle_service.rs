use crate::trading::model::market::candles::CandlesModel;
use okx::dto::market_dto::CandleOkxRespDto;

pub struct CandleService {}
impl CandleService {
    pub fn new() -> Self {
        Self {}
    }
    pub async fn update_candle(
        &self,
        candle: Vec<CandleOkxRespDto>,
        inst_id: &str,
        time_interval: &str,
    ) -> anyhow::Result<()> {
        let candle_model = CandlesModel::new().await;
        candle_model
            .update_or_create(&candle.get(0).unwrap(), inst_id, time_interval)
            .await?;
        Ok(())
    }
}
