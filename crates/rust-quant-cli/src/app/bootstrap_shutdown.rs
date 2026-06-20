/// 设置多种退出信号处理
async fn setup_shutdown_signals() -> &'static str {
    use tokio::signal;

    #[cfg(unix)]
    {
        let mut sigterm = match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                error!("❌ 注册 SIGTERM 失败: {}", e);
                return "SIGNAL_SETUP_FAILED";
            }
        };
        let mut sigint = match signal::unix::signal(signal::unix::SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                error!("❌ 注册 SIGINT 失败: {}", e);
                return "SIGNAL_SETUP_FAILED";
            }
        };
        let mut sigquit = match signal::unix::signal(signal::unix::SignalKind::quit()) {
            Ok(s) => s,
            Err(e) => {
                error!("❌ 注册 SIGQUIT 失败: {}", e);
                return "SIGNAL_SETUP_FAILED";
            }
        };

        // 注意：tokio 的 unix Signal::recv() 返回 Option<()>。
        // 在极少数情况下（底层 stream 被关闭）会立刻返回 None，如果不处理会导致程序“无信号也退出”。
        loop {
            tokio::select! {
                v = sigterm.recv() => {
                    if v.is_some() {
                        break "SIGTERM";
                    }
                    warn!("⚠️ SIGTERM 信号流已关闭，继续等待其他信号");
                }
                v = sigint.recv() => {
                    if v.is_some() {
                        break "SIGINT";
                    }
                    warn!("⚠️ SIGINT 信号流已关闭，继续等待其他信号");
                }
                v = sigquit.recv() => {
                    if v.is_some() {
                        break "SIGQUIT";
                    }
                    warn!("⚠️ SIGQUIT 信号流已关闭，继续等待其他信号");
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        if let Err(e) = signal::ctrl_c().await {
            error!("❌ 监听 CTRL+C 失败: {}", e);
            return "SIGNAL_SETUP_FAILED";
        }
        "CTRL+C"
    }
}
