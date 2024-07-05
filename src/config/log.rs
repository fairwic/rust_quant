use std::env;
use dotenv::dotenv;
use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, FmtSubscriber, Layer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// 设置日志
pub async fn setup_logging() -> anyhow::Result<()> {
    dotenv().ok();
    let app_env = env::var("APP_ENV").expect("app_env config is none");
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

        tracing_subscriber::registry()
            .with(fmt::layer().with_writer(info_non_blocking).with_filter(EnvFilter::new("info")))
            .with(fmt::layer().with_writer(error_non_blocking).with_filter(EnvFilter::new("error")))
            .init();
    }

    /// enable log crate to show sql logs
    // fast_log::init(fast_log::Config::new().console().level(log::LevelFilter::Debug)).expect("fast_log init error");
    // if let Err(e) = fast_log::init(Config::new().console()) {
    //     eprintln!("fast_log init error: {:?}", e);
    // }

    Ok(())
}
