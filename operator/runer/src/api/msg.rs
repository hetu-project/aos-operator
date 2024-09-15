use super::vrf_key::VRFPrivKey;
use crate::operator::OperatorArc;
use alloy::primitives::Address;
use alloy_wrapper::contracts::vrf_range;
use hex::FromHex;
use node_api::config::OperatorConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;
use verify_hub::opml::{
    handler::opml_question_handler,
    model::{OpmlAnswer, OpmlAnswerResponse, OpmlRequest},
};
use verify_hub::tee::{
    handler::tee_question_handler,
    model::{AnswerReq, Params, QuestionReq},
};
use websocket::{
    connect, ReceiveMessage, WebsocketConfig, WebsocketReceiver, WebsocketSender, WireMessage,
};

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
    pub tag: String,
    pub clock: HashMap<String, String>,
    pub position: String,
    pub job_id: String,
    pub job: Job,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Job {
    pub model: String,
    pub prompt: String,
    pub tag: String,
    pub params: JobParams,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobParams {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DispatchJobResponse {
    pub code: u16,
    pub message: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct JobResultRequest(pub Vec<JobResultParam>);

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct JobResultParam {
    pub job_id: String,
    pub result: String,
    pub clock: HashMap<String, String>,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerResponse {
    pub code: u16,
    pub result: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerResult {
    pub answer: String,
    pub model: String,
    pub node_id: String,
    pub prompt: String,
    pub req_id: String,
    pub state_root: String,
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
    let job_id = params.job_id.clone();
    info!("dispatch params:{params:?}");
    let (clk_id, clk_val) = params.clock.get_key_value("1").unwrap();
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

    let mut res: JobResultRequest = Default::default();
    match params.job.tag.as_str() {
        "opml" => {
            let opml_request = OpmlRequest {
                model: "llama-7b".to_owned(),
                prompt: "what's ai".to_owned(),
                req_id: "".to_owned(),
                callback: "http://127.0.0.1:21001/api/opml_callback".to_owned(),
            };

            let qest = opml_question_handler(state.unwrap(), opml_request)
                .await
                .unwrap();
            let worker_response: WorkerResponse = serde_json::from_value(qest).unwrap();
            let worker_result: OpmlAnswer = serde_json::from_value(worker_response.result).unwrap();

            res = JobResultRequest(vec![JobResultParam {
                job_id: job_id,
                result: worker_result.answer,
                clock: HashMap::from([(
                    clk_id.to_owned(),
                    clk_val
                        .parse::<u16>()
                        .unwrap()
                        .checked_add(1)
                        .unwrap()
                        .to_string(),
                )]),
                operator: String::from_str("operator1").unwrap(),
                signature: String::from_str("signature").unwrap(),
            }]);
        }
        "tee" => {
            let tee_request = QuestionReq {
                message: "1".to_owned(),
                message_id: "1".to_owned(),
                conversation_id: "1".to_owned(),
                model: "1".to_owned(),
                params: Params {
                    temperature: 1.0,
                    top_p: 2.0,
                    max_tokens: 3,
                },
                callback_url: "1".to_owned(),
            };
            //tee_question_handler();

            let qest = tee_question_handler(state.unwrap(), tee_request)
                .await
                .unwrap();
            let worker_response: WorkerResponse = serde_json::from_value(qest).unwrap();
            let worker_result: AnswerReq = serde_json::from_value(worker_response.result).unwrap();

            res = JobResultRequest(vec![JobResultParam {
                job_id: job_id,
                result: worker_result.answer,
                clock: HashMap::from([(
                    clk_id.to_owned(),
                    clk_val
                        .parse::<u16>()
                        .unwrap()
                        .checked_add(1)
                        .unwrap()
                        .to_string(),
                )]),
                operator: String::from_str("operator1").unwrap(),
                signature: String::from_str("signature").unwrap(),
            }]);
        }
        "zk" => panic!("no way"),
        _ => panic!("no way"),
    }

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
    //let socket = SocketAddr::from_str("13.215.49.139:3000").unwrap();
    //let socket = SocketAddr::from_str("127.0.0.1:8080").unwrap();
    let mut retry_count = 0;
    loop {
        //let socket = SocketAddr::from_str("127.0.0.1:8081").unwrap();
        let socket = SocketAddr::from_str("13.215.49.139:3000").unwrap();
        match connect(Arc::new(WebsocketConfig::default()), socket).await {
            Ok((sender, mut receiver)) => {
                let (tx, rx) = mpsc::channel(1);

                let r_task = tokio::task::spawn(async move {
                    //while let Ok(msg) = receiver.recv().await {
                    loop {
                        match receiver.recv().await {
                            Ok(msg) => {
                                info!("<-- {:?}", msg);
                                retry_count = 0;

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
                            Err(_e) => {
                                info!("here ---");
                                break;
                            }
                        }
                    }
                });

                connect_dispatcher(&sender).await;

                let _operator_clone = _operator.clone();
                let s_task = tokio::task::spawn(async move {
                    handle_dispatchjobs(_operator_clone, sender, rx).await;
                });
                s_task.await;
                r_task.await;
            }
            Err(_) => {
                retry_count += 1;
                let delay = (2_u64).pow(retry_count);
                println!("Failed to connect, retrying in {} seconds", delay);
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }
        }
    }
}
