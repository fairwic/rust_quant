use std::env;
use dotenv::dotenv;
use fast_log::Config;
use tokio::sync::Mutex;
use tracing::{Event, Level, Subscriber};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, FmtSubscriber, Layer, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use std::sync::Arc;

// 定义一个自定义的 Layer
struct CustomLayer {
    event_count: Arc<Mutex<u32>>,
}

impl<S> Layer<S> for CustomLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event, _: tracing_subscriber::layer::Context<S>) {
        let event_count = Arc::clone(&self.event_count);
        let level = *event.metadata().level();
        let event_message = format!("Event received: {:?}", event);

        // 在异步任务中处理事件
        tokio::spawn(async move {
            let mut count = event_count.lock().await;
            *count += 1;
            if level == Level::ERROR {
                // todo: 日志事件发送到远程服务器、记录到数据库或触发告警
                println!("收到Error级别错误: {:?}", event_message);
            }
        });
    }
}

// 设置日志
pub async fn setup_logging() -> anyhow::Result<()> {
    dotenv().ok();
    let app_env = env::var("APP_ENV").expect("app_env config is none");

    let custom_layer = CustomLayer {
        event_count: Arc::new(Mutex::new(0)),
    };

    if app_env == "LOCAL" {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let info_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "info.log");
        let error_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "error.log");

        let (info_non_blocking, _info_guard) = tracing_appender::non_blocking(info_file);
        let (error_non_blocking, _error_guard) = tracing_appender::non_blocking(error_file);

        let subscriber = Registry::default()
            .with(fmt::layer().with_writer(info_non_blocking).with_filter(EnvFilter::new("info")))
            .with(fmt::layer().with_writer(error_non_blocking).with_filter(EnvFilter::new("error")))
            .with(custom_layer);

        tracing::subscriber::set_global_default(subscriber)?;
    }
    /// enable log crate to show sql logs
    // fast_log::init(fast_log::Config::new().console().level(log::LevelFilter::Debug)).expect("fast_log init error");
    // if let Err(e) = fast_log::init(Config::new().console()) {
    //     eprintln!("fast_log init error: {:?}", e);
    // }
    Ok(())
}