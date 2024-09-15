use crate::server::server::SharedState;
use crate::tee::model::*;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{debug_handler, extract, BoxError, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::borrow::Cow;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use uuid::uuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonResponse {
    code: u16,
    result: String,
}

#[debug_handler]
pub async fn tee_question_handler(
    State(server): State<SharedState>,
    req: Json<QuestionReq>,
) -> Json<serde_json::Value> {
    tracing::info!("Handling question {:?}", req);

    let uuid = uuid::Uuid::new_v4();
    let request_id = uuid.to_string();
    {
        let mut server = server.0.write().await;

        //let mut conn = server.pg.get().expect("Failed to get a connection from pool");
        //let q = create_question(&mut conn, request_id.clone(), req.message.clone(), req.message_id.clone(), req.conversation_id.clone(), req.model.clone(), req.callback_url.clone());

        tracing::info!("request_id: {}", request_id);

        let op_req = OperatorReq {
            request_id: "1".to_owned(), //TODO
            node_id: "".to_string(),
            model: req.model.clone(),
            prompt: req.message.clone(),
            prompt_hash: "".to_string(),
            signature: "".to_string(),
            params: req.params.clone(),
        };

        let work_name = server
            .tee_operator_collections
            .keys()
            .next()
            .unwrap()
            .clone();
        server.send_tee_inductive_task(work_name, op_req).await;
    }

    let (tx, mut rx) = mpsc::channel(1);
    {
        let mut server = server.0.write().await;
        server.tee_channels.insert(request_id.clone(), tx);
    }

    // Poll the database for the answer
    match tokio::time::timeout(Duration::from_secs(600), rx.recv()).await {
        Ok(Some(answer)) => Json(json!({
            "code": 200,
            "result": answer
        })),
        _ => {
            // Clean up the channel if we time out
            let mut server = server.0.write().await;
            server.tee_channels.remove(&request_id);

            Json(json!({
                "code": 408,
                "result": "Request timed out"
            }))
        }
    }
}

#[debug_handler]
pub async fn register_worker(
    State(server): State<SharedState>,
    Json(req): Json<Operator>,
) -> Json<RegisterResp> {
    tracing::info!("Registering worker {:?}", req);
    let mut server = server.0.write().await;
    server.add_worker(
        req.worker_name.clone(),
        req.check_heart_beat,
        req.worker_status.clone(),
        req.multimodal,
    );

    let response = RegisterResp {
        code: 200,
        result: "ok".to_string(),
    };
    Json(response)
}

#[debug_handler]
pub async fn receive_heart_beat(
    State(server): State<SharedState>,
    Json(req): Json<HeartBeatReq>,
) -> Json<HeartBeatResp> {
    tracing::info!("Receiving heart beat {:?}", req);
    let mut server = server.0.write().await;
    let exist = server
        .tee_operator_collections
        .contains_key(&req.worker_name);
    let response = HeartBeatResp { exist };
    Json(response)
}

#[debug_handler]
pub async fn tee_callback(
    State(server): State<SharedState>,
    Json(req): Json<AnswerReq>,
) -> Json<AnswerResp> {
    tracing::info!("tee_callback function triggered: {:?}", req);

    let server = server.0.read().await;
    //let mut conn = server.pg.get().expect("Failed to get a connection from pool");

    // Forward the answer to the callback URL
    if let Some(tx) = server.tee_channels.get(&req.request_id) {
        tracing::info!(
            "Sending answer through channel, request_id: {}",
            req.request_id
        );
        if let Err(e) = tx.send(req.clone()).await {
            tracing::error!("Failed to send OPML answer through channel: {:?}", e);
        }
    }

    let response = AnswerResp {
        code: 200,
        result: "Callback stored successfully".to_string(),
    };

    Json(response)
}

pub async fn handle_error(error: BoxError) -> impl IntoResponse {
    if error.is::<tower::timeout::error::Elapsed>() {
        return (StatusCode::REQUEST_TIMEOUT, Cow::from("request timed out"));
    }

    if error.is::<tower::load_shed::error::Overloaded>() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Cow::from("service is overloaded, try again later"),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Cow::from(format!("Unhandled internal error: {error}")),
    )
}
