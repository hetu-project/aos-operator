use alloy::{
    primitives::keccak256,
    signers::{local::PrivateKeySigner, Signature as AlloySignature, SignerSync},
};
use hex::FromHex;
use node_api::error::OperatorAPIResult;
use secp256k1::SecretKey;
use serde_json::Value;
use signer::msg_signer::{MessageVerify, Signer};
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::info;
use websocket::{
    ReceiveMessage, WebsocketConfig, WebsocketListener, WebsocketReceiver, WebsocketSender,
};

use tracing::*;
use tracing_subscriber::EnvFilter;

use super::msg::{DispatchJobParam, DispatchJobRequest, Job, JobParams};

fn handle_jobs(msg: Value) -> OperatorAPIResult<Value> {
    Ok(serde_json::to_value(super::msg::WsResponse {
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
        let s = WebsocketListener::bind(Arc::new(WebsocketConfig::default()), "127.0.0.1:8081")
            .await
            .unwrap();

        let addr = s.local_addr().unwrap();
        info!("server: {}", addr);

        let secret_key = PrivateKeySigner::from_slice(
            &<[u8; 32]>::from_hex(
                "77f4b2fbf3f32687f03d84d323bd5cb443f53b0fc338b51c24e319a520c87217",
            )
            .unwrap_or_default(),
        )
        .expect("signer err");

        let signer = MessageVerify(secret_key);
        let (send, mut recv) = s.accept(signer).await.unwrap();

        let r_task = tokio::task::spawn(async move {
            handle_connection(recv).await;
        });

        let mut map = std::collections::HashMap::new();
        map.insert("1".to_string(), "2".to_owned());
        for i in (1..=10) {
            let _res = send
                .request_timeout(
                    String::from_str("dispatch_job").unwrap(),
                    serde_json::to_value(DispatchJobRequest(vec![DispatchJobParam {
                        user: String::from_str("AI").unwrap(),
                        seed: String::from_str("abc").unwrap(),
                        signature: String::from_str("signature").unwrap(),
                        tag: "malicious".to_owned(),
                        clock: map.clone(),
                        position: "before".to_owned(),
                        job_id: String::from_str("100").unwrap(),
                        job: Job {
                            model: String::from_str("2").unwrap(),
                            prompt: String::from_str("what's AI?").unwrap()
                                + i.to_string().as_str(),
                            tag: "opml".to_owned(),
                            params: JobParams {
                                temperature: 1.0,
                                top_p: 5.0,
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
            thread::sleep(Duration::from_secs(5));
        }

        r_task.await.unwrap();
    });
    s_task.await.unwrap();
}
