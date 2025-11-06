use std::env;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, mpsc};
use tracing::{info, Event, Level, Subscriber};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{fmt, EnvFilter, FmtSubscriber, Layer, Registry};

// 修改导入路径
use crate::config::email;
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

        email::send_email(&email_title, email_body).await;
    }

    fn generate_error_key(error_message: &str) -> String {
        // 简单的错误分类：提取关键信息用于去重
        // 使用 Unicode 字符边界安全截断，避免在多字节字符中间截断
        const MAX_LEN: usize = 100;
        
        if error_message.len() > MAX_LEN {
            // 找到安全的截断点（不在字符中间）
            let mut end = MAX_LEN;
            while end > 0 && !error_message.is_char_boundary(end) {
                end -= 1;
            }
            error_message[..end].to_string()
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

// 全局变量用于保持日志文件句柄
use std::sync::OnceLock;

static INFO_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();
static ERROR_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

// 日志配置结构体
#[derive(Debug, Clone)]
struct LogConfig {
    app_env: String,
    log_level: String,
    log_dir: String,
    log_rotation: String,
    info_file_name: String,
    error_file_name: String,
    enable_file_logging: bool,
    enable_console_logging: bool,
    enable_email_notification: bool,
    max_file_size: u64,
    max_files: usize,
}

impl LogConfig {
    fn from_env() -> anyhow::Result<Self> {
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "local".to_string());
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let log_dir = env::var("LOG_DIR").unwrap_or_else(|_| "log_files".to_string());
        let log_rotation = env::var("LOG_ROTATION").unwrap_or_else(|_| "daily".to_string());
        let info_file_name = env::var("LOG_INFO_FILE").unwrap_or_else(|_| "info.log".to_string());
        let error_file_name = env::var("LOG_ERROR_FILE").unwrap_or_else(|_| "error.log".to_string());
        let enable_file_logging = env::var("ENABLE_FILE_LOGGING")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);
        let enable_console_logging = env::var("ENABLE_CONSOLE_LOGGING")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);
        let enable_email_notification = env::var("ENABLE_EMAIL_NOTIFICATION")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);
        let max_file_size = env::var("LOG_MAX_FILE_SIZE_MB")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .unwrap_or(100);
        let max_files = env::var("LOG_MAX_FILES")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30);

        Ok(Self {
            app_env,
            log_level,
            log_dir,
            log_rotation,
            info_file_name,
            error_file_name,
            enable_file_logging,
            enable_console_logging,
            enable_email_notification,
            max_file_size,
            max_files,
        })
    }
}

// 解析时间轮转策略
fn parse_rotation(s: &str) -> Rotation {
    match s.to_lowercase().as_str() {
        "minutely" | "minute" | "min" => Rotation::MINUTELY,
        "hourly" | "hour" | "hr" => Rotation::HOURLY,
        "daily" | "day" => Rotation::DAILY,
        _ => Rotation::DAILY,
    }
}

// 设置日志
pub async fn setup_logging() -> anyhow::Result<()> {
    let config = LogConfig::from_env()?;

    let custom_layer_opt = if config.enable_email_notification {
        Some(CustomLayer { event_count: Arc::new(Mutex::new(0)) })
    } else { None };

    // 本地环境：仅控制台输出
    if config.app_env == "local" {
        let base = Registry::default()
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
                    .with_filter(EnvFilter::new(&config.log_level)),
            );

        if let Some(custom) = custom_layer_opt {
            tracing::subscriber::set_global_default(base.with(custom))?;
        } else {
            tracing::subscriber::set_global_default(base)?;
        }

        info!("Log configuration setup successfully!");
        info!("Environment: {}, Log Level: {}, File Logging: {}, Console Logging: {}",
              config.app_env, config.log_level, false, true);
        return Ok(());
    }

    // 非本地环境：文件输出（可选控制台）
    std::fs::create_dir_all(&config.log_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create log directory '{}': {}", config.log_dir, e))?;

    let rotation_info = parse_rotation(&config.log_rotation);
    let rotation_error = parse_rotation(&config.log_rotation);

    let info_file = RollingFileAppender::new(rotation_info, &config.log_dir, &config.info_file_name);
    let error_file = RollingFileAppender::new(rotation_error, &config.log_dir, &config.error_file_name);

    let (info_non_blocking, info_guard) = tracing_appender::non_blocking(info_file);
    let (error_non_blocking, error_guard) = tracing_appender::non_blocking(error_file);

    // 保存guard到全局，防止被丢弃
    INFO_GUARD.set(info_guard).map_err(|_| anyhow::anyhow!("Failed to set INFO_GUARD"))?;
    ERROR_GUARD.set(error_guard).map_err(|_| anyhow::anyhow!("Failed to set ERROR_GUARD"))?;

    let base = Registry::default()
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
                .with_filter(EnvFilter::new(&config.log_level)),
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
        );

    // 按需添加控制台层与自定义层，并立即设置全局订阅者，避免类型不一致
    if config.enable_console_logging {
        let with_console = base.with(
            fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_level(true)
                .with_writer(std::io::stdout)
                .with_filter(EnvFilter::new(&config.log_level)),
        );
        if let Some(custom) = custom_layer_opt {
            tracing::subscriber::set_global_default(with_console.with(custom))?;
        } else {
            tracing::subscriber::set_global_default(with_console)?;
        }
    } else {
        if let Some(custom) = custom_layer_opt {
            tracing::subscriber::set_global_default(base.with(custom))?;
        } else {
            tracing::subscriber::set_global_default(base)?;
        }
    }

    info!("Log configuration setup successfully!");
    info!("Environment: {}, Log Level: {}, File Logging: {}, Console Logging: {}",
          config.app_env, config.log_level, true, config.enable_console_logging);
    Ok(())
}
