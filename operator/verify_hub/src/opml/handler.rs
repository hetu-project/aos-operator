use crate::error;
use crate::opml::model::*;
use crate::server::server::SharedState;
use axum::{debug_handler, extract::State, Json};
use tokio::sync::mpsc;
use tokio::time::Duration;

pub async fn opml_question_handler(
    server: SharedState,
    opml_request: OpmlRequest,
) -> Result<OpmlAnswer, error::VerifyHubError> {
    let req_id = opml_request.req_id.clone();

    {
        let mut tserver = server.0.write().await;
        // Send the request to the OPML server
        if let Err(e) = tserver.send_opml_request(opml_request).await {
            tracing::error!("Failed to send OPML request: {:?}", e);
            return Err(error::VerifyHubError::SendOpmlRequestError(format!(
                "{:?}",
                e
            )));
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
    //let ret = rx.recv().await;
    //tracing::info!("Received OPML answer, req_id: {}", req_id);
    //ret.ok_or(error::VerifyHubError::ChannelClosedError)

        match tokio::time::timeout(Duration::from_secs(600), rx.recv()).await {
            Ok(Some(answer)) => Ok(answer),
            _ => {
                // Clean up the channel if we time out
                //let mut server =
                //    server.0.write().await;
                //server.tee_channels.remove(&request_id);

                Err(error::VerifyHubError::RequestTimeoutError)
            }
        }
}

#[debug_handler]
pub async fn opml_callback(
    State(server): State<SharedState>,
    Json(req): Json<OpmlAnswer>,
) -> Json<OpmlAnswerResponse> {
    tracing::info!("Handling OPML answer: {:?}", req);

    let mut tserver = server.0.write().await;

    tracing::info!(
        "id:{:?}, tx:{:?}",
        &req.req_id,
        tserver.opml_channels.get(&req.req_id)
    );
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
