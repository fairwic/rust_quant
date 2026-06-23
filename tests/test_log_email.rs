use rust_quant::app_init;
use tracing::error;
use tokio::time::{sleep, Duration};
#[tokio::test]
async fn test_log_email_trigger() -> anyhow::Result<()> {
    // 初始化应用
    app_init().await?;
    println!("🚀 开始测试优化后的日志邮件系统");
    // 模拟高并发错误日志
    println!("📧 模拟高并发错误场景...");
    for i in 1..=20 {
        error!("高并发测试错误 #{}: 数据库连接失败", i);
        if i % 5 == 0 {
            error!("高并发测试错误 #{}: 网络超时", i);
        }
        if i % 10 == 0 {
            error!("高并发测试错误 #{}: 内存不足", i);
        }
    }
    println!("⏰ 等待批量处理...");
    // 等待批量处理（默认60秒间隔，这里等待65秒确保处理完成）
    sleep(Duration::from_secs(65)).await;
    println!("✅ 高并发测试完成，应该收到1封汇总邮件而不是20封单独邮件");
    Ok(())
}
#[tokio::test]
async fn test_email_deduplication() -> anyhow::Result<()> {
    // 初始化应用
    app_init().await?;
    println!("🔄 测试邮件去重功能");
    // 发送相同的错误日志多次
    for i in 1..=10 {
        error!("重复错误: 数据库连接失败 - 连接池耗尽");
        sleep(Duration::from_millis(100)).await;
    }
    println!("⏰ 等待去重处理...");
    sleep(Duration::from_secs(65)).await;
    println!("✅ 去重测试完成，10个相同错误应该合并为1个条目");
    Ok(())
}
