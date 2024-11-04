use crate::error;
use crate::server::server::SharedState;
use crate::zkml::model::*;
use axum::{debug_handler, extract::State, Json};
use tokio::sync::mpsc;
use tokio::time::Duration;

pub async fn zkml_question_handler(
    server: SharedState,
    zkml_request: ZkmlRequest,
) -> Result<ZkmlAnswer, error::VerifyHubError> {
    let req_id = zkml_request.req_id.clone();

    let mut tserver = server.0.write().await;
    // Send the request to the Zkml server
    match tserver.send_zkml_request(zkml_request).await {
        Ok(answer) => Ok(answer),
        Err(e) => Err(error::VerifyHubError::SendZkmlRequestError(e.to_string()))
    }

    //let (tx, mut rx) = mpsc::channel(1);

    //{
    //    let mut tserver = server.0.write().await;
    //    // Create a channel for receiving the answer
    //    tserver.zkml_channels.insert(req_id.clone(), tx);
    //}

    //// Poll the channel for the answer from the callback
    //tracing::info!("Waiting for ZKML answer, req_id: {}", req_id);
    ////let ret = rx.recv().await;
    ////tracing::info!("Received ZkML answer, req_id: {}", req_id);
    ////ret.ok_or(error::VerifyHubError::ChannelClosedError)

    //match tokio::time::timeout(Duration::from_secs(600), rx.recv()).await {
    //    Ok(Some(answer)) => Ok(answer),
    //    _ => {
    //        // Clean up the channel if we time out
    //        //let mut server =
    //        //    server.0.write().await;
    //        //server.tee_channels.remove(&request_id);

    //        Err(error::VerifyHubError::RequestTimeoutError)
    //    }
    //}
}

#[debug_handler]
pub async fn zkml_callback(
    State(server): State<SharedState>,
    Json(req): Json<ZkmlAnswer>,
) -> Json<ZkmlAnswerResponse> {
    tracing::info!("Handling ZkML answer: {:?}", req);

    let mut tserver = server.0.write().await;

    tracing::info!(
        "id:{:?}, tx:{:?}",
        &req.req_id,
        tserver.zkml_channels.get(&req.req_id)
    );
    // Send the answer through the channel if it exists
    if let Some(tx) = tserver.zkml_channels.get(&req.req_id) {
        tracing::info!(
            "Sending ZKML answer through channel, req_id: {}",
            req.req_id
        );
        if let Err(e) = tx.send(req.clone()).await {
            tracing::error!("Failed to send ZKML answer through channel: {:?}", e);
        }
    }

    let response = ZkmlAnswerResponse {
        code: 200,
        result: "ZKML answer stored successfully".to_string(),
    };
    Json(response)
}
