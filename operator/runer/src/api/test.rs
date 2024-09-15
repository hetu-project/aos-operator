use node_api::error::OperatorAPIResult;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;
use websocket::{
    ReceiveMessage, WebsocketConfig, WebsocketListener, WebsocketReceiver, WebsocketSender,
};

use tracing::*;
use tracing_subscriber::EnvFilter;

use super::msg::{DispatchJobParam, DispatchJobRequest, Job, JobParams};

fn handle_jobs(msg: Value) -> OperatorAPIResult<Value> {
    Ok(serde_json::to_value(super::msg::ConnectResponse {
        code: 200,
        message: String::from_str("success2").unwrap(),
    })
    .unwrap())
}

pub async fn handle_connection(mut receiver: WebsocketReceiver) {
    while let Ok(msg) = receiver.recv().await {
        info!("Received message: {:?}", msg);

        match msg {
            ReceiveMessage::Signal(s) => println!("{s:?}"),
            ReceiveMessage::Request(id, m, p, r) => {
                let rep = match m.as_str() {
                    "connect" => handle_jobs(p),
                    "job_result" => handle_jobs(p),
                    &_ => panic!("todo"),
                }
                .unwrap();
                if let Err(e) = r.respond(rep).await {
                    info!("Failed to send message: {:?}", e);
                    return;
                }
            }
        };
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test() {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(rust_log))
        .init();
    let s_task = tokio::task::spawn(async move {
        let s = WebsocketListener::bind(Arc::new(WebsocketConfig::default()), "127.0.0.1:8080")
            .await
            .unwrap();

        let addr = s.local_addr().unwrap();
        info!("server: {}", addr);

        let (send, mut recv) = s.accept().await.unwrap();

        let r_task = tokio::task::spawn(async move {
            handle_connection(recv).await;
        });

        let mut map = std::collections::HashMap::new();
        map.insert("1".to_string(), "2".to_owned());
        let _res = send
            .request_timeout(
                String::from_str("dispatch_job").unwrap(),
                serde_json::to_value(DispatchJobRequest(vec![DispatchJobParam {
                    user: String::from_str("AI").unwrap(),
                    seed: String::from_str("abc").unwrap(),
                    signature: String::from_str("signature").unwrap(),
                    clock: map,
                    job_id: String::from_str("1").unwrap(),
                    job: Job {
                        model: String::from_str("2").unwrap(),
                        prompt: String::from_str("3").unwrap(),
                        params: JobParams {
                            temperature: 1.0,
                            top_p: String::from_str("5").unwrap(),
                            max_tokens: 100,
                        },
                    },
                }]))
                .unwrap(),
                std::time::Duration::from_secs(5),
            )
            .await
            .unwrap();

        info!("{:?}", _res);

        r_task.await.unwrap();
    });
    s_task.await.unwrap();
}
