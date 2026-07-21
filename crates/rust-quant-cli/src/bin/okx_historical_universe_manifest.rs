use anyhow::Result;
use rust_quant_cli::app::okx_historical_universe::{
    generate_historical_universe_manifest, parse_historical_universe_args,
};

/// 生成只读 Research DatasetManifest，不保存回测结果，也不触发交易路径。
#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_historical_universe_args(std::env::args().skip(1))?;
    let output = args.output.clone();
    let manifest = generate_historical_universe_manifest(&args).await?;
    println!(
        "okx_historical_universe_manifest: version={} months={} output={}",
        manifest.universe_version,
        manifest.months.len(),
        output.display()
    );
    Ok(())
}
