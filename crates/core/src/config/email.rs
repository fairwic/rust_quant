use std::env;
use std::time::Duration;

use lettre::message::header;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// 邮件发送配置
#[derive(Debug, Clone)]
pub struct EmailConfig {
    /// SMTP 超时时间（秒）
    pub smtp_timeout_secs: u64,
    /// 总体超时时间（秒）
    pub total_timeout_secs: u64,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_timeout_secs: 10,  // SMTP 命令超时 10 秒
            total_timeout_secs: 15, // 总体超时 15 秒
        }
    }
}

/// 优化后的邮件发送函数 - 非阻塞，带超时控制
pub async fn send_email(title: &str, body: String) {
    send_email_with_config(title, body, EmailConfig::default()).await;
}

/// 带配置的邮件发送函数
pub async fn send_email_with_config(title: &str, body: String, config: EmailConfig) {
    let title = title.to_string(); // 转换为 owned String

    // 在独立的阻塞任务中执行邮件发送，避免阻塞异步运行时
    let result =
        tokio::task::spawn_blocking(move || send_email_blocking(&title, body, config)).await;

    match result {
        Ok(Ok(())) => {
            println!("Email sent successfully!");
        }
        Ok(Err(e)) => {
            eprintln!("Could not send email: {:?}", e);
        }
        Err(e) => {
            eprintln!("Email task panicked: {:?}", e);
        }
    }
}

/// 同步阻塞的邮件发送实现（在独立线程中运行）
fn send_email_blocking(
    title: &str,
    body: String,
    config: EmailConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // SMTP 服务器地址和端口
    let smtp_server =
        env::var("EMAIL_SMTP_SERVER").unwrap_or_else(|_| "smtp.gmail.com".to_string());
    let smtp_port = env::var("EMAIL_SMTP_PORT").unwrap_or_else(|_| "587".to_string());

    // 发件人和收件人
    let from = env::var("EMAIL_FROM").unwrap_or_else(|_| "xxxxxxxx@gmail.com".to_string());
    let to = env::var("EMAIL_TO").unwrap_or_else(|_| "xxxxxx@163.com".to_string());

    // 发件人邮箱的凭证
    let username =
        env::var("EMAIL_SEND_USERNAME").unwrap_or_else(|_| "xxxxxxxx@gmail.com".to_string());
    let password = env::var("EMAIL_SEND_PASSWORD").unwrap_or_else(|_| "xxxxxx".to_string());

    // 创建邮件内容
    let email = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject(title)
        .header(header::ContentType::TEXT_PLAIN)
        .body(body)?;

    // 设置 SMTP 客户端，配置超时
    let creds = Credentials::new(username, password);

    let mailer = SmtpTransport::starttls_relay(&smtp_server)?
        .port(smtp_port.parse()?)
        .credentials(creds)
        .timeout(Some(Duration::from_secs(config.smtp_timeout_secs))) // 🔧 设置 SMTP 超时
        .build();

    // 发送邮件
    mailer.send(&email)?;

    Ok(())
}
