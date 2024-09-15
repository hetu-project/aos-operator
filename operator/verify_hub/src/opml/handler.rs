use crate::opml::model::*;
use crate::server::server::SharedState;
use crate::tee::model::{AnswerResp, QuestionReq};
use axum::{debug_handler, extract::State, Json};
use chrono::Utc;
use diesel::associations::HasTable;
use diesel::{PgConnection, RunQueryDsl};
use std::time::Duration as StdDuration;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

pub async fn opml_question_handler(
    server: SharedState,
    mut opml_request: OpmlRequest,
) -> Option<serde_json::Value> {
    static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    opml_request.req_id = id.to_string();

    //let opml_request = OpmlRequest {
    //    model:  "llama-7b".to_owned(),
    //    prompt: "hello".to_owned(),
    //    req_id: "1".to_owned(),
    //    callback: "http://127.0.0.1:21001/api/opml_callback".to_owned(),
    //};
    let req_id = opml_request.req_id.clone();


    {
    let mut tserver = server.0.write().await;
    // Send the request to the OPML server
    if let Err(e) = tserver.send_opml_request(opml_request).await {
        tracing::error!("Failed to send OPML request: {:?}", e);
        return Some(serde_json::json!({
            "code": 500,
            "result": "Failed to send OPML request"
        }));
    }
    }

    let (tx, mut rx) = mpsc::channel(1);

    {
    let mut tserver = server.0.write().await;
    // Create a channel for receiving the answer
    tserver.opml_channels.insert(req_id.clone(), tx);
    }

    // Poll the channel for the answer from the callback
    tracing::info!("Waiting for OPML answer, req_id: {}", req_id);
    let ret = rx.recv().await;
    tracing::info!("Received OPML answer, req_id: {}", req_id);
    match ret {
        Some(answer) => Some(serde_json::json!({
            "code": 200,
            "result": answer
        })),
        None => Some(serde_json::json!({
            "code": 500,
            "result": "Channel closed unexpectedly"
        })),
    }
}

#[debug_handler]
pub async fn opml_callback(
    State(server): State<SharedState>,
    Json(req): Json<OpmlAnswer>,
) -> Json<OpmlAnswerResponse> {
    tracing::info!("Handling OPML answer: {:?}", req);

    let mut tserver = server.0.write().await;

    tracing::info!("id:{:?}, tx:{:?}", &req.req_id, tserver.opml_channels.get(&req.req_id));
    // Send the answer through the channel if it exists
    if let Some(tx) = tserver.opml_channels.get(&req.req_id) {
        tracing::info!(
            "Sending OPML answer through channel, req_id: {}",
            req.req_id
        );
        if let Err(e) = tx.send(req.clone()).await {
            tracing::error!("Failed to send OPML answer through channel: {:?}", e);
        }
    }

    let response = OpmlAnswerResponse {
        code: 200,
        result: "OPML answer stored successfully".to_string(),
    };
    Json(response)
}
