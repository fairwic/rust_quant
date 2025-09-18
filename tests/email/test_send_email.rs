use rust_quant::app_config::email::send_email;
use tokio::time::Duration;
use tracing::error;  

#[tokio::test]
async fn test_send_email() -> anyhow::Result<()> {
    rust_quant::app_init().await?;
    let res:Result<(),anyhow::Error>=Err(anyhow::anyhow!("test"));
    error!("rust_quant ERROR:{:?}",res);
    // send_email("test", t".to_string()g()).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    Ok(())
}
#[tokio::test]
async fn test_send_email2() -> anyhow::Result<()> {
    rust_quant::app_init().await?;
    send_email("test", "test".to_string()).await;
    Ok(())
}