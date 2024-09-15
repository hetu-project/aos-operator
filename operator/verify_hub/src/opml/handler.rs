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
    opml_request: OpmlRequest,
) -> Json<serde_json::Value> {
    let mut tserver = server.0.write().await;
    let req_id = opml_request.req_id.clone();

    // Send the request to the OPML server
    if let Err(e) = tserver.send_opml_request(opml_request).await {
        tracing::error!("Failed to send OPML request: {:?}", e);
        return Json(serde_json::json!({
            "code": 500,
            "result": "Failed to send OPML request"
        }));
    }

    let (tx, mut rx) = mpsc::channel(1);

    let mut server = server.0.write().await;
    // Create a channel for receiving the answer
    server.opml_channels.insert(req_id.clone(), tx);

    // Poll the channel for the answer from the callback
    tracing::info!("Waiting for OPML answer, req_id: {}", req_id);
    let ret = rx.recv().await;
    tracing::info!("Received OPML answer, req_id: {}", req_id);
    match ret {
        Some(answer) => Json(serde_json::json!({
            "code": 200,
            "result": answer
        })),
        None => Json(serde_json::json!({
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

    let mut server = server.0.write().await;

    // Send the answer through the channel if it exists
    if let Some(tx) = server.opml_channels.get(&req.req_id) {
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
