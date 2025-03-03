use async_trait::async_trait;
#[async_trait]
pub trait TopContractServiceTrait {
    //在当前时间戳上加上指定的周期
    async fn init(&self, inst_ids: Vec<&str>, periods: Vec<&str>) -> anyhow::Result<()>;
    async fn sync(&self, inst_ids: Vec<&str>, periods: Vec<&str>) -> anyhow::Result<()>;

    fn ts_add_n_period(ts: i64, period: &str) -> anyhow::Result<Option<(i64, i64)>> {
        let begin = crate::time_util::ts_add_n_period(ts, period, 1)?;
        let end = crate::time_util::ts_add_n_period(ts, period, 101)?;
        Ok(Some((begin, end)))
    }
}
