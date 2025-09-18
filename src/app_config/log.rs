use std::env;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, mpsc};
use tracing::{Event, Level, Subscriber};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, EnvFilter, FmtSubscriber, Layer, Registry};

use crate::app_config;
use fast_log::Config;

// 邮件发送配置
#[derive(Debug, Clone)]
struct EmailConfig {
    /// 批量发送间隔（秒）
    batch_interval_secs: u64,
    /// 最大批量大小
    max_batch_size: usize,
    /// 去重时间窗口（秒）
    dedup_window_secs: u64,
    /// 最大队列大小
    max_queue_size: usize,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            batch_interval_secs: 60,    // 1分钟批量发送一次
            max_batch_size: 10,         // 最多10个错误合并发送
            dedup_window_secs: 300,     // 5分钟内相同错误去重
            max_queue_size: 1000,       // 最大队列1000个
        }
    }
}

// 错误日志条目
#[derive(Debug, Clone)]
struct ErrorLogEntry {
    message: String,
    timestamp: Instant,
    count: usize,
}

// 邮件发送器
struct EmailSender {
    config: EmailConfig,
    error_queue: Arc<Mutex<HashMap<String, ErrorLogEntry>>>,
    sender: mpsc::UnboundedSender<String>,
}

impl EmailSender {
    fn new(config: EmailConfig) -> Self {
        let error_queue = Arc::new(Mutex::new(HashMap::new()));
        let (sender, mut receiver) = mpsc::unbounded_channel::<String>();

        let queue_clone = Arc::clone(&error_queue);
        let config_clone = config.clone();

        // 启动批量发送任务
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config_clone.batch_interval_secs));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::process_batch_emails(&queue_clone, &config_clone).await;
                    }
                    Some(error_msg) = receiver.recv() => {
                        Self::add_error_to_queue(&queue_clone, error_msg, &config_clone).await;
                    }
                }
            }
        });

        Self {
            config,
            error_queue,
            sender,
        }
    }

    async fn add_error_to_queue(
        queue: &Arc<Mutex<HashMap<String, ErrorLogEntry>>>,
        error_message: String,
        config: &EmailConfig,
    ) {
        let mut queue_guard = queue.lock().await;

        // 检查队列大小
        if queue_guard.len() >= config.max_queue_size {
            // 移除最旧的条目
            if let Some(oldest_key) = queue_guard.iter()
                .min_by_key(|(_, entry)| entry.timestamp)
                .map(|(k, _)| k.clone()) {
                queue_guard.remove(&oldest_key);
            }
        }

        // 生成错误的哈希键（用于去重）
        let error_key = Self::generate_error_key(&error_message);

        match queue_guard.get_mut(&error_key) {
            Some(entry) => {
                // 更新现有条目
                entry.count += 1;
                entry.timestamp = Instant::now();
            }
            None => {
                // 添加新条目
                queue_guard.insert(error_key, ErrorLogEntry {
                    message: error_message,
                    timestamp: Instant::now(),
                    count: 1,
                });
            }
        }
    }

    async fn process_batch_emails(
        queue: &Arc<Mutex<HashMap<String, ErrorLogEntry>>>,
        config: &EmailConfig,
    ) {
        let mut queue_guard = queue.lock().await;

        if queue_guard.is_empty() {
            return;
        }

        // 收集要发送的错误（限制批量大小）
        let mut errors_to_send = Vec::new();
        let mut keys_to_remove = Vec::new();

        for (key, entry) in queue_guard.iter() {
            if errors_to_send.len() >= config.max_batch_size {
                break;
            }

            errors_to_send.push(entry.clone());
            keys_to_remove.push(key.clone());
        }

        // 移除已处理的条目
        for key in &keys_to_remove {
            queue_guard.remove(key);
        }

        drop(queue_guard); // 释放锁

        if !errors_to_send.is_empty() {
            Self::send_batch_email(errors_to_send).await;
        }
    }

    async fn send_batch_email(errors: Vec<ErrorLogEntry>) {
        let total_errors: usize = errors.iter().map(|e| e.count).sum();
        let email_title = format!("系统错误报告 - 共{}个错误", total_errors);

        let mut email_body = format!("在过去的时间窗口内，系统发生了{}个错误：\n\n", total_errors);

        for (i, error) in errors.iter().enumerate() {
            email_body.push_str(&format!(
                "{}. 错误信息: {}\n   发生次数: {}\n   最后发生时间: {:?}\n\n",
                i + 1,
                error.message,
                error.count,
                error.timestamp
            ));
        }

        app_config::email::send_email(&email_title, email_body).await;
    }

    fn generate_error_key(error_message: &str) -> String {
        // 简单的错误分类：提取关键信息用于去重
        // 可以根据需要实现更复杂的分类逻辑
        if error_message.len() > 100 {
            error_message[..100].to_string()
        } else {
            error_message.to_string()
        }
    }

    async fn send_error(&self, error_message: String) {
        if let Err(_) = self.sender.send(error_message) {
            eprintln!("Failed to send error to email queue");
        }
    }
}

// 全局邮件发送器
static EMAIL_SENDER: tokio::sync::OnceCell<EmailSender> = tokio::sync::OnceCell::const_new();

async fn get_email_sender() -> &'static EmailSender {
    EMAIL_SENDER.get_or_init(|| async {
        EmailSender::new(EmailConfig::default())
    }).await
}

// 定义一个自定义的 Layer
struct CustomLayer {
    event_count: Arc<Mutex<u32>>,
}

impl<S> Layer<S> for CustomLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event, _: tracing_subscriber::layer::Context<S>) {
        let level = *event.metadata().level();

        if level == Level::ERROR {
            let event_message = format!("{:?}", event);

            // 使用优化的邮件发送器
            tokio::spawn(async move {
                let email_sender = get_email_sender().await;
                email_sender.send_error(event_message).await;
            });
        }

        // 更新计数器（可选，用于监控）
        let event_count = Arc::clone(&self.event_count);
        tokio::spawn(async move {
            let mut count = event_count.lock().await;
            *count += 1;
        });
    }
}

// 设置日志
pub async fn setup_logging() -> anyhow::Result<()> {
    let app_env = env::var("APP_ENV").expect("app_env config is none");

    let custom_layer = CustomLayer {
        event_count: Arc::new(Mutex::new(0)),
    };

    if app_env == "local" {
        let subscriber = Registry::default()
            .with(
                fmt::layer()
                    .with_ansi(true)
                    .with_target(false)
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_level(true)
                    .with_writer(std::io::stdout)
                    .with_filter(EnvFilter::new("info")),
            )
            .with(custom_layer);  // 在local环境下也添加CustomLayer
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let info_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "info.log");
        let error_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "error.log");

        let (info_non_blocking, _info_guard) = tracing_appender::non_blocking(info_file);
        let (error_non_blocking, _error_guard) = tracing_appender::non_blocking(error_file);

        let subscriber = Registry::default()
            .with(
                fmt::layer()
                    .with_ansi(false)
                    .with_target(false)
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_level(true)
                    .with_writer(info_non_blocking)
                    .with_filter(EnvFilter::new("info")),
            )
            .with(
                fmt::layer()
                    .with_ansi(false)
                    .with_target(false)
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_level(true)
                    .with_writer(error_non_blocking)
                    .with_filter(EnvFilter::new("error")),
            )
            .with(custom_layer);

        tracing::subscriber::set_global_default(subscriber)?;
    }

    if "true" == env::var("DB_DEBUG").unwrap_or_default() {
        // fast_log::init(
        //     fast_log::Config::new()
        //         .console()
        //         .level(log::LevelFilter::Info),
        // )
        // .expect("fast_log init error");
    }
    // enable log crate to show sql logs
    // if let Err(e) = fast_log::init(Config::new().console()) {
    //     eprintln!("fast_log init error: {:?}", e);
    // }
    Ok(())
}
