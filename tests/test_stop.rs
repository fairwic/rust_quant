use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

// ... 上文的 background_task 定义 ...
// ... 上文的 shutdown_signal 定义 ...

async fn hello() -> &'staticstr {
    "Hello, World!"
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(hello));

    // 这是下班铃的遥控器
    let cancellation_token = CancellationToken::new();
    let token_clone = cancellation_token.clone();

    // 派一个厨子去后台干活，并给他一个下班铃分机
    let background_handle = tokio::spawn(background_task(token_clone));

    // 监听端口并启动服务（Axum 0.7 写法）
    let listener = TcpListener::bind(("127.0.0.1", 3000)).await.unwrap();
    println!("餐厅开张，监听在 {:?}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    // 服务员下班后，按响遥控器，通知厨子下班
    cancellation_token.cancel();

    // 等厨子打扫完厨房
    if let Err(e) = background_handle.await {
        eprintln!("厨子下班路上出了点问题: {}", e);
    }

    println!("餐厅完美打烊，所有人都安全回家！");
}