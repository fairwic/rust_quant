use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

pub mod back_test;
pub mod okx;
pub mod test_predicting;
pub mod email;
pub mod test_nwe;
pub mod test_nwe_strategy;

#[tokio::test]
async fn test_mspc_job() -> anyhow::Result<()> {
    // 创建一个mpsc通道
    // let (tx, rx) = mpsc::channel();
    // 创建一个所
    let key = Arc::new(Mutex::new(0));

    let mut handles = vec![];

    for i in 0..10 {
        // let tx2 = tx.clone();
        let key = key.clone();
        let handle = thread::spawn(move || {
            let mut key = key.lock().unwrap();
            *key += 1;
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
    println!("result:{}", *key.lock().unwrap());
    // let handle = thread::spawn(move || {
    // for i in 0..10 {
    // tx.send(i.to_string()).unwrap();
    // }
    // });
    //等待所有线程完成
    // handle.join().unwrap();
    // 显式丢弃原始发送者，这样当所有线程结束后通道会关闭
    Ok(())
}
