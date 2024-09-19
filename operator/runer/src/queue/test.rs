use super::queue::msg_queue::{MessageQueue, RedisMessage, RedisStreamPool};
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio;
use tracing::*;
use tracing_subscriber::EnvFilter;

mod queue;

#[tokio::test(flavor = "multi_thread")]
async fn test() {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(rust_log))
        .init();

    info!("start test");
    let mut queue = RedisStreamPool::new("redis://:@127.0.0.1:6379/")
        .await
        .unwrap();

    let queue_clone = queue.clone();
    let s_task = tokio::task::spawn(async move {
        let msg = RedisMessage::new("Hello, world");
        info!("Product message: {:?}", msg);
        queue_clone.produce("opml", &msg).await.unwrap();
    });

    let r_task = tokio::task::spawn(async move {
        let msgs = queue.consume("opml").await.unwrap();

        for (k, m) in msgs.iter().enumerate() {
            let recover: String = serde_json::from_str(m.data.as_str()).unwrap();
            info!("Consumed message: id={:?}, data={:?}", m.id, recover);
            //assert_eq!(m.data, serde_json::from_slice(res.as_slice()).unwrap());
            queue.acknowledge("opml", &m.id).await.unwrap();
        }
    });
    s_task.await.unwrap();
    r_task.await.unwrap();
}
