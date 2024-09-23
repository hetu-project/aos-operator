use crate::error;
use crate::server::server::SharedState;
use crate::tee::model::*;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{debug_handler, BoxError, Json};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use tokio::sync::mpsc;
use tokio::time::Duration;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonResponse {
    code: u16,
    result: String,
}

pub async fn tee_question_handler(
    server: SharedState,
    req: QuestionReq,
) -> Result<AnswerReq, error::VerifyHubError> {
    tracing::info!("Handling question {:?}", req);

    let uuid = Uuid::new_v4();
    let request_id = uuid.to_string();
    {
        let mut server = server.0.write().await;

        tracing::info!("request_id: {}", request_id);

        let op_req = OperatorReq {
            request_id: request_id.clone(),
            node_id: "".to_string(),
            model: req.model.clone(),
            prompt: req.message.clone(),
            prompt_hash: "".to_string(),
            signature: "".to_string(),
            params: req.params.clone(),
        };

        server.send_tee_inductive_task(op_req).await;
    }

    let (tx, mut rx) = mpsc::channel(1);
    {
        let mut server = server.0.write().await;
        server.tee_channels.insert(request_id.clone(), tx);
    }

    // Poll the database for the answer
    match tokio::time::timeout(Duration::from_secs(600), rx.recv()).await {
        Ok(Some(answer)) => Ok(answer),
        _ => {
            // Clean up the channel if we time out
            let mut server = server.0.write().await;
            server.tee_channels.remove(&request_id);

            Err(error::VerifyHubError::RequestTimeoutError)
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
