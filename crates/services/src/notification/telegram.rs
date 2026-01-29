use anyhow::Result;
use reqwest::Client;
use serde::Serialize;
use tracing::{error, info};

/// Telegram Bot 通知服务
pub struct TelegramNotifier {
    client: Client,
    bot_token: String,
    chat_id: String,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
}

impl TelegramNotifier {
    /// 从环境变量创建通知器
    /// 需要设置: TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID
    pub fn from_env() -> Result<Self> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| anyhow::anyhow!("TELEGRAM_BOT_TOKEN not set"))?;
        let chat_id = std::env::var("TELEGRAM_CHAT_ID")
            .map_err(|_| anyhow::anyhow!("TELEGRAM_CHAT_ID not set"))?;

        Ok(Self {
            client: Client::new(),
            bot_token,
            chat_id,
        })
    }

    /// 发送文本消息 (Markdown 格式)
    pub async fn send_message(&self, text: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        let request = SendMessageRequest {
            chat_id: &self.chat_id,
            text,
            parse_mode: "Markdown",
        };

        let response = self.client.post(&url).json(&request).send().await?;

        if response.status().is_success() {
            info!("📨 Telegram message sent successfully");
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to send Telegram message: {} - {}", status, body);
            Err(anyhow::anyhow!("Telegram API error: {}", status))
        }
    }

    /// 发送排名变化通知
    pub async fn notify_rank_change(
        &self,
        symbol: &str,
        timeframe: &str,
        old_rank: i32,
        new_rank: i32,
        delta: i32,
    ) -> Result<()> {
        let emoji = if delta > 0 { "🚀" } else { "📉" };
        let direction = if delta > 0 { "上升" } else { "下降" };

        let message = format!(
            "{} *排名剧变*\n\n\
             *币种*: `{}`\n\
             *周期*: {}\n\
             *排名*: {} → {} ({}{} 名)\n",
            emoji,
            symbol,
            timeframe,
            old_rank,
            new_rank,
            if delta > 0 { "+" } else { "" },
            delta
        );

        self.send_message(&message).await
    }

    /// 发送 Top 150 进出通知
    pub async fn notify_list_change(&self, symbol: &str, is_entry: bool, rank: i32) -> Result<()> {
        let (emoji, action) = if is_entry {
            ("🔔", "进入 Top 150")
        } else {
            ("⚠️", "跌出 Top 150")
        };

        let message = format!(
            "{} *榜单变动*\n\n\
             *币种*: `{}`\n\
             *事件*: {}\n\
             *当前排名*: #{}\n",
            emoji, symbol, action, rank
        );

        self.send_message(&message).await
    }
}
