use okx::websocket::auto_reconnect_client::AutoReconnectWebsocketClient;
use rust_quant_market::streams::deep_stream_manager::DeepStreamManager;
use rust_quant_services::market::FlowAnalyzer;
use tokio::sync::Mutex;

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

fn main() {
    println!("Checking AutoReconnectWebsocketClient...");
    assert_send::<AutoReconnectWebsocketClient>();
    assert_sync::<AutoReconnectWebsocketClient>();
    println!("AutoReconnectWebsocketClient is Send + Sync");

    println!("Checking Mutex<AutoReconnectWebsocketClient>...");
    assert_send::<Mutex<AutoReconnectWebsocketClient>>();
    assert_sync::<Mutex<AutoReconnectWebsocketClient>>();
    println!("Mutex<AutoReconnectWebsocketClient> is Send + Sync");

    println!("Checking DeepStreamManager...");
    assert_send::<DeepStreamManager>();
    assert_sync::<DeepStreamManager>();
    println!("DeepStreamManager is Send + Sync");

    println!("Checking FlowAnalyzer...");
    assert_send::<FlowAnalyzer>();
    // FlowAnalyzer doesn't need to be Sync, just Send for tokio::spawn
    println!("FlowAnalyzer is Send");
}
