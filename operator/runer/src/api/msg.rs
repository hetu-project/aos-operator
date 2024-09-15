use super::vrf_key::VRFPrivKey;
use crate::operator::OperatorArc;
use alloy::primitives::Address;
use alloy_wrapper::contracts::vrf_range;
use hex::FromHex;
use node_api::config::OperatorConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;
use verify_hub::opml::{handler::opml_question_handler, model::OpmlRequest};
use websocket::{ReceiveMessage, WebsocketReceiver, WebsocketSender, WireMessage};

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectRequest(pub Vec<ConnectParam>);

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectParam {
    pub operator: String,
    pub hash: String,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectResponse {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DispatchJobRequest(pub Vec<DispatchJobParam>);

#[derive(Serialize, Deserialize, Debug)]
pub struct DispatchJobParam {
    pub user: String,
    pub seed: String,
    pub signature: String,
    pub clock: HashMap<String, String>,
    pub job_id: String,
    pub job: Job,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Job {
    pub model: String,
    pub prompt: String,
    pub params: JobParams,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobParams {
    pub temperature: f64, //TODO String
    pub top_p: String,
    pub max_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DispatchJobResponse {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobResultRequest(pub Vec<JobResultParam>);

#[derive(Serialize, Deserialize, Debug)]
pub struct JobResultParam {
    pub job_id: String,
    pub result: String,
    pub operator: String,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobResultResponse {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WsResponse {
    pub code: u16,
    pub message: String,
}

pub async fn connect_dispatcher(sender: &WebsocketSender) {
    let _res = sender
        .request_timeout(
            String::from_str("connect").unwrap(),
            serde_json::to_value(&ConnectRequest(vec![ConnectParam {
                operator: String::from_str("operator101").unwrap(),
                hash: String::from_str("abc").unwrap(),
                signature: String::from_str("signature").unwrap(),
            }]))
            .unwrap(),
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();

    match serde_json::from_value(_res).unwrap() {
        ConnectResponse { code, message } => {
            info!("<-- code: {:?}, message: {:?}", code, message);
        }
        _ => panic!("no way"),
    };
}

pub async fn job_result(sender: &WebsocketSender, msg: Value) {
    let _res = sender
        .request_timeout(
            String::from_str("job_result").unwrap(),
            msg,
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();

    match serde_json::from_value(_res).unwrap() {
        ConnectResponse { code, message } => {
            info!("<-- code: {:?}, message: {:?}", code, message);
        }
        _ => panic!("no way"),
    };
}

//TODO
async fn demo(_msg: DispatchJobRequest, tx: oneshot::Sender<Value>, op: OperatorArc) {
    let config = op.lock().await.config.clone();
    let contract = op.lock().await.vrf_range_contract.clone();
    let vrf_key = op.lock().await.vrf_key.clone();
    let state = op.lock().await.hub_state.clone();

    let bytes = <[u8; 20]>::from_hex(&config.node.node_id[2..]).unwrap_or_default();
    let addr: Address = Address::new(bytes);
    let (start, end) = vrf_range::get_range_by_address2(contract, addr)
        .await
        .unwrap();

    //TODO loop
    let params = &_msg.0[0];

    let range = match params.user.as_str() {
        "suspicion" => (start, end),
        "malicious" => (start, end),
        _ => (start, end),
    };

    let vrf_threshold = end - start;
    let vrf_precision = config.chain.vrf_sort_precision as usize;
    let vrf_prompt_hash = params.job.prompt.clone();

    let _res = vrf_key.run_vrf(vrf_prompt_hash, vrf_precision, vrf_threshold);
    info!("vrf={:?}", _res);

    let opml_request = OpmlRequest {
        model: "llama".to_owned(),
        prompt: "what's ai".to_owned(),
        req_id: "1".to_owned(),
        callback: "url".to_owned(),
    };
    opml_question_handler(state.unwrap(), opml_request).await;

    let res = JobResultRequest(vec![JobResultParam {
        job_id: String::from_str("1").unwrap(),
        result: String::from_str("abc").unwrap(),
        operator: String::from_str("operator1").unwrap(),
        signature: String::from_str("signature").unwrap(),
    }]);

    tx.send(serde_json::to_value(&res).unwrap()).unwrap();
}

async fn do_job(id: String, _job: DispatchJobRequest, tx: WebsocketSender, op: OperatorArc) {
    let (_tx, rx) = oneshot::channel();

    //TODO store in pg
    let _task = tokio::task::spawn(async move {
        //TODO call verify hub function
        demo(_job, _tx, op).await;
    });

    let msg: Value = rx.await.unwrap();

    job_result(&tx, msg).await;
}

async fn handle_dispatchjobs(
    operator: OperatorArc,
    sender: WebsocketSender,
    mut rx: mpsc::Receiver<(String, Value)>,
) {
    while let Some((id, msg)) = rx.recv().await {
        let job: DispatchJobRequest = serde_json::from_value(msg).unwrap();
        info!("{:?}", job);

        let op = operator.clone();

        let sender_clone = sender.clone();
        let _task = tokio::task::spawn(async move {
            do_job(id, job, sender_clone, op).await;
        });
    }
}

pub async fn handle_connection(_operator: OperatorArc) {
    let sender = _operator.lock().await.sender.take().unwrap();
    let mut receiver = _operator.lock().await.receiver.take().unwrap();

    let (tx, rx) = mpsc::channel(1);

    let r_task = tokio::task::spawn(async move {
        while let Ok(msg) = receiver.recv().await {
            info!("<-- {:?}", msg);

            match msg {
                ReceiveMessage::Signal(s) => println!("{s:?}"),
                ReceiveMessage::Request(id, m, p, r) => {
                    match m.as_str() {
                        "dispatch_job" => tx.send((id, p)).await.unwrap(),
                        &_ => panic!("on way"),
                    };

                    let rep = serde_json::to_value(DispatchJobResponse {
                        code: 200,
                        message: String::from_str("success").unwrap(),
                    })
                    .unwrap();

                    if let Err(e) = r.respond(rep).await {
                        println!("Failed to send message: {:?}", e);
                        return;
                    }
                }
            };
        }
    });

    connect_dispatcher(&sender).await;

    let s_task = tokio::task::spawn(async move {
        handle_dispatchjobs(_operator, sender, rx).await;
    });

    r_task.await.unwrap();
    s_task.await.unwrap();
}
