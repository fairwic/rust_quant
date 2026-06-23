#[derive(Debug, Clone, PartialEq, Eq)]
struct DuneSyncRequest {
    /// 类型标识。
    metric_type: String,
    /// 交易对或资产符号。
    symbol: String,
    /// template路径，用于构建接口请求。
    template_path: String,
    /// 键值扩展数据。
    params: HashMap<String, String>,
    /// performance，用于构建接口请求。
    performance: DuneQueryPerformance,
}
/// 封装当前函数，减少配置运行时调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn run_dune_sync_jobs_from_env() -> Result<()> {
    let envs: HashMap<String, String> = std::env::vars().collect();
    let requests = parse_dune_sync_requests_from_map(&envs)?;
    if requests.is_empty() {
        return Ok(());
    }
    for request in requests {
        info!(
            "📊 执行Dune模板同步: metric_type={}, symbol={}, template_path={}",
            request.metric_type, request.symbol, request.template_path
        );
        ExternalMarketSyncJob::sync_dune_template(
            &request.metric_type,
            &request.symbol,
            &request.template_path,
            request.params,
            request.performance,
        )
        .await?;
    }
    Ok(())
}
/// 解析输入参数并收敛为 配置、基础设施和运行时 可使用的结构化值。
fn parse_dune_sync_requests_from_map(
    envs: &HashMap<String, String>,
) -> Result<Vec<DuneSyncRequest>> {
    if !env_flag_is_true(envs, "IS_RUN_DUNE_SYNC_JOB") {
        return Ok(Vec::new());
    }
    if let Some(raw_jobs) = envs
        .get("DUNE_TEMPLATE_JOBS")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let mut requests = Vec::new();
        for job in raw_jobs
            .split(';')
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
        {
            let parts: Vec<&str> = job.split('|').map(|item| item.trim()).collect();
            if parts.len() < 6 || parts.len() > 7 {
                return Err(anyhow!(
                    "DUNE_TEMPLATE_JOBS 格式错误，期望 metric_type|symbol|template_path|start_time|end_time|performance|[min_usd]: {}",
                    job
                ));
            }
            let min_usd = parts.get(6).copied().unwrap_or("100000");
            requests.push(build_dune_sync_request(
                parts[0], parts[1], parts[2], parts[3], parts[4], parts[5], min_usd,
            )?);
        }
        return Ok(requests);
    }
    Ok(vec![build_dune_sync_request(
        env_required(envs, "DUNE_METRIC_TYPE")?.as_str(),
        env_required(envs, "DUNE_SYMBOL")?.as_str(),
        env_required(envs, "DUNE_TEMPLATE_PATH")?.as_str(),
        env_required(envs, "DUNE_START_TIME")?.as_str(),
        env_required(envs, "DUNE_END_TIME")?.as_str(),
        envs.get("DUNE_PERFORMANCE")
            .map(String::as_str)
            .unwrap_or("medium"),
        envs.get("DUNE_MIN_USD")
            .map(String::as_str)
            .unwrap_or("100000"),
    )?])
}
/// 构建 配置、基础设施和运行时 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_dune_sync_request(
    metric_type: &str,
    symbol: &str,
    template_path: &str,
    start_time: &str,
    end_time: &str,
    performance: &str,
    min_usd: &str,
) -> Result<DuneSyncRequest> {
    let mut params = HashMap::new();
    params.insert("symbol".to_string(), symbol.to_string());
    params.insert("start_time".to_string(), start_time.to_string());
    params.insert("end_time".to_string(), end_time.to_string());
    params.insert("min_usd".to_string(), min_usd.to_string());
    Ok(DuneSyncRequest {
        metric_type: metric_type.to_string(),
        symbol: symbol.to_string(),
        template_path: template_path.to_string(),
        params,
        performance: parse_dune_performance(performance)?,
    })
}
/// 解析输入参数并收敛为 配置、基础设施和运行时 可使用的结构化值。
fn parse_dune_performance(value: &str) -> Result<DuneQueryPerformance> {
    match value.to_ascii_lowercase().as_str() {
        "medium" => Ok(DuneQueryPerformance::Medium),
        "large" => Ok(DuneQueryPerformance::Large),
        other => Err(anyhow!("不支持的 Dune performance: {}", other)),
    }
}
/// 封装环境变量必需，减少配置运行时调用方重复实现相同细节。
fn env_required(envs: &HashMap<String, String>, key: &str) -> Result<String> {
    envs.get(key)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("缺少环境变量 {}", key))
}
