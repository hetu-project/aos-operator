use super::msg::*;
use tokio::time::{sleep, Duration};
use super::vrf_key::VRFReply;
use crate::operator::OperatorArc;
use crate::queue::msg_queue::{MessageQueue, RedisMessage, RedisStreamPool};
use alloy::primitives::Address;
use alloy::signers::local::yubihsm::setup::Report;
use alloy::{
    primitives::keccak256,
    signers::{local::PrivateKeySigner, Signature as AlloySignature, SignerSync},
};
use alloy_wrapper::contracts::vrf_range;
use ed25519_dalek::{Digest, Sha512};
use hex::FromHex;
use secp256k1::SecretKey;
use serde_json::Value;
use signer::msg_signer::{MessageVerify, Signer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
use verify_hub::{
    opml::{
        handler::opml_question_handler,
        model::{OpmlAnswer, OpmlRequest},
    },
    server::server::SharedState,
    tee::{
        handler::tee_question_handler,
        model::{AnswerReq, OperatorReq, Params},
    },
};
use websocket::{connect, ReceiveMessage, WebsocketConfig, WebsocketSender};

pub async fn connect_dispatcher(sender: &WebsocketSender, node_id: &str) {
    let res = sender
        .request_timeout(
            String::from_str("connect").unwrap(),
            serde_json::to_value(&ConnectRequest(vec![ConnectParam {
                operator: node_id.to_owned(),
                hash: "".to_owned(),
                signature: "".to_owned(),
            }]))
            .unwrap(),
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();

    match serde_json::from_value(res).unwrap() {
        WsResponse { code, message } => {
            info!("<--connect respond: code={:?}, message={:?}", code, message);
        }
    };
}

pub async fn job_result(sender: &WebsocketSender, msg: Value) {
    let res = sender
        .request_timeout(
            String::from_str("job_result").unwrap(),
            msg,
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();

    match serde_json::from_value(res).unwrap() {
        WsResponse { code, message } => {
            info!("<--JobResult: code={:?}, message={:?}", code, message);
        }
    };
}

async fn do_opml_job(
    state: SharedState,
    model: String,
    prompt: String,
    user: String,
    job_id: String,
    tag: String,
    callback: String,
    clock: &HashMap<String, String>,
) -> JobResultRequest {

    let mut retry_count = 0;
    loop {
        let status = get_worker_status().await.unwrap();
        info!("worker status: {:?}", status);
        if status == "0" {
            break;
        } 

        if (retry_count == 360) {
            panic!("opml worker timeout...");
        }

        sleep(Duration::from_millis(10000)).await;
        retry_count += 1;
    }

    let opml_request = OpmlRequest {
        model,
        prompt,
        req_id: Uuid::new_v4().to_string(),
        callback: callback,
    };

    let worker_result: OpmlAnswer = opml_question_handler(state, opml_request).await.unwrap();

    let mut key_value_vec: Vec<(&String, &String)> = clock.iter().collect();
    let (clk_id, clk_val) = key_value_vec
        .pop()
        .map(|(id, val)| (id.as_str(), val))
        .unwrap();

    JobResultRequest(vec![JobResultParam {
        user,
        job_id,
        tag,
        //result: worker_result.answer[..=worker_result.answer.rfind('.').unwrap()].to_string(),
        result: worker_result.answer,
        vrf: VRFReply::default(),
        clock: HashMap::from([(
            clk_id.to_owned(),
            clk_val
                .parse::<u16>()
                .unwrap()
                .checked_add(1)
                .unwrap()
                .to_string(),
        )]),
        operator: "".to_owned(),
        signature: "".to_owned(),
    }])
}

async fn do_tee_job(
    state: SharedState,
    model: String,
    prompt: String,
    user: String,
    job_id: String,
    tag: String,
    temperature: f64,
    top_p: f64,
    max_tokens: u64,
    clock: &HashMap<String, String>,
) -> JobResultRequest {
    let tee_request = OperatorReq {
        request_id: Uuid::new_v4().to_string(),
        node_id: "".to_string(),
        model: model.clone(),
        prompt: prompt.clone(),
        prompt_hash: "".to_string(),
        signature: "".to_string(),
        params: Params {
            temperature: temperature as f32,
            top_p: top_p as f32,
            max_tokens: max_tokens as u16,
        },
    };

    let worker_result: AnswerReq = tee_question_handler(state, tee_request).await.unwrap();

    let mut key_value_vec: Vec<(&String, &String)> = clock.iter().collect();
    let (clk_id, clk_val) = key_value_vec
        .pop()
        .map(|(id, val)| (id.as_str(), val))
        .unwrap();

    JobResultRequest(vec![JobResultParam {
        user,
        job_id,
        tag,
        result: worker_result.answer,
        vrf: VRFReply::default(),
        clock: HashMap::from([(
            clk_id.to_owned(),
            clk_val
                .parse::<u16>()
                .unwrap()
                .checked_add(1)
                .unwrap()
                .to_string(),
        )]),
        operator: "".to_owned(),
        signature: "".to_owned(),
    }])
}

async fn compute_vrf(
    op: OperatorArc,
    tag: String,
    position: String,
    prompt: String,
) -> Result<VRFReply, std::io::Error> {
    let config = op.lock().await.config.clone();
    let contract = op.lock().await.vrf_range_contract.clone();
    let vrf_key = op.lock().await.vrf_key.clone();
    let addr: Address =
        Address::new(<[u8; 20]>::from_hex(&config.node.node_id[2..]).unwrap_or_default());

    let (start, end) = vrf_range::get_range_by_address(contract.clone(), addr)
        .await
        .map(|(x, y)| (x as f64, y as f64))
        .unwrap();
    info!("start-{start:?}, end-{end:?}");
    let max_range = vrf_range::get_max_range(contract)
        .await
        .or_else(|_e| Ok::<u64, Report>(0xffffffff))
        .unwrap() as f64;

    //adjust acording to tag and position
    let range = match tag.as_str() {
        "suspicion" => (
            (start - max_range * 0.03) as u64,
            (end + max_range * 0.03) as u64,
        ),
        "malicious" => match position.as_str() {
            "before" => (
                (start - max_range * 0.1) as u64,
                (end + max_range * 0.1) as u64,
            ),
            "after" => (
                (start - max_range * 0.03) as u64,
                (end + max_range * 0.03) as u64,
            ),
            &_ => panic!("Unexpected value: {}", position),
        },
        _ => (start as u64, end as u64),
    };

    info!("start2 -{max_range:?},star-{:?} end-{:?}", range.0, range.1);

    let vrf_threshold = range.1 - range.0;
    let vrf_precision = config.chain.vrf_sort_precision as usize;
    let mut hasher = Sha512::new();
    Digest::update(&mut hasher, prompt);
    let vrf_prompt_hash = hasher
        .finalize()
        .to_vec()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect();

    vrf_key.run_vrf(vrf_prompt_hash, vrf_precision, vrf_threshold)
}

async fn do_job(msg: DispatchJobRequest, sender: WebsocketSender, op: OperatorArc) {
    let config = op.lock().await.config.clone();
    let state = op.lock().await.hub_state.clone();
    let storage = op.lock().await.storage.clone();

    //for params in &msg.0.iter() {}
    let params = &msg.0[0];
    let job_id = params.job_id.clone();
    info!("dispatch params:{params:?}");

    let position = params.position.clone();
    let user = params.user.clone();

    let vrf_result = compute_vrf(op, params.tag.clone(), position, params.job.prompt.clone())
        .await
        .unwrap();
    info!("vrf re: {:?}", vrf_result);
    let mut res = if vrf_result.selected == false {
        let mut key_value_vec: Vec<(&String, &String)> = params.clock.iter().collect();
        let (clk_id, clk_val) = key_value_vec
            .pop()
            .map(|(id, val)| (id.as_str(), val))
            .unwrap();

        JobResultRequest(vec![JobResultParam {
            user,
            job_id,
            tag: params.tag.clone(),
            result: "".to_owned(),
            vrf: vrf_result,
            clock: HashMap::from([(
                clk_id.to_owned(),
                clk_val
                    .parse::<u16>()
                    .unwrap()
                    .checked_add(1)
                    .unwrap()
                    .to_string(),
            )]),
            operator: "".to_owned(),
            signature: "".to_owned(),
        }])
    } else {
        match params.job.tag.as_str() {
            "opml" => {
                do_opml_job(
                    state.unwrap(),
                    params.job.model.to_owned(),
                    params.job.prompt.clone(),
                    user,
                    job_id,
                    params.tag.clone(),
                    config.net.callback_url.clone(),
                    &params.clock,
                )
                .await
            }
            "tee" => {
                do_tee_job(
                    state.unwrap(),
                    params.job.model.to_owned(),
                    params.job.prompt.clone(),
                    user,
                    job_id,
                    params.tag.clone(),
                    params.job.params.temperature,
                    params.job.params.top_p,
                    params.job.params.max_tokens,
                    &params.clock,
                )
                .await
            }
            _ => panic!("Unexpected value: {}", params.job.tag),
        }
    };

    storage
        .sinker_job(
            res.0[0].job_id.clone(),
            res.0[0].user.clone(),
            res.0[0].result.clone(),
            res.0[0].tag.clone(),
            serde_json::to_string(&res.0[0].clock).unwrap(),
        )
        .await;

    job_result(&sender, serde_json::to_value(&res).unwrap()).await;
}

async fn handle_dispatchjobs(
    operator: OperatorArc,
    sender: WebsocketSender,
    mut queue: RedisStreamPool,
) {
    let config = operator.lock().await.config.clone();
    let node_type = config.node.node_type.clone();
    loop {
        let msgs = queue.consume("opml").await.unwrap();

        for (_k, m) in msgs.iter().enumerate() {
            //TODO worker if free?
            //
            let (id, msg): (String, Value) = serde_json::from_str(m.data.as_str()).unwrap();
            info!("Consumed message: id={:?}, data={:?}", id, msg);

            let job: DispatchJobRequest = serde_json::from_value(msg).unwrap();
            if job.0[0].job.tag != node_type {
                info!("unsupport operaoter : {:?}", job.0[0].job.tag);
            } else {
                let op = operator.clone();
                let sender_clone = sender.clone();

                do_job(job, sender_clone, op).await;
            }
            queue.acknowledge("opml", &m.id).await.unwrap();
        }
    }
}

pub async fn handle_connection(op: OperatorArc) {
    let config = op.lock().await.config.clone();
    let queue = RedisStreamPool::new(&config.queue.queue_url).await.unwrap();
    let mut retry_count = 0;
    let retry_max = 8;

    loop {
        let secret_key = PrivateKeySigner::from_slice(
            &<[u8; 32]>::from_hex(&config.node.signer_key).unwrap_or_default(),
        )
        .expect("32 bytes, within curve order");
        let signer = MessageVerify(secret_key);
        let socket = SocketAddr::from_str(&config.net.dispatcher_url).unwrap();
        match connect(Arc::new(WebsocketConfig::default()), socket, signer).await {
            Ok((sender, mut receiver)) => {
                retry_count = 0;
                let queue_clone = queue.clone();
                let r_task = tokio::task::spawn(async move {
                    loop {
                        match receiver.recv().await {
                            Ok(msg) => {
                                info!("<--job request: msg={:?}", msg);

                                match msg {
                                    ReceiveMessage::Signal(s) => println!("{s:?}"),
                                    ReceiveMessage::Request(id, m, p, r) => {
                                        match m.as_str() {
                                            "dispatch_job" => {
                                                sleep(Duration::from_millis(10)).await;
                                                let redis_msg = RedisMessage::new((id, p)).unwrap();
                                                info!("Product message: data={:?}", redis_msg);
                                                queue_clone
                                                    .produce("opml", &redis_msg)
                                                    .await
                                                    .unwrap();
                                            }
                                            &_ => panic!("Unexpected value"),
                                        };

                                        let rep = serde_json::to_value(WsResponse {
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
                                panic!("{:?}", _e);
                            }
                        }
                    }
                });

                connect_dispatcher(&sender, config.node.node_id.as_str()).await;

                let _operator_clone = op.clone();
                let queue_cl = queue.clone();
                let s_task = tokio::task::spawn(async move {
                    handle_dispatchjobs(_operator_clone, sender, queue_cl).await;
                });

                //if let Err(e) = futures::future::try_join_all(vec![s_task, r_task]).await {
                //    info!("error {:?}", e);
                //}

                tokio::select! {
                        result = s_task =>match result {
                              Ok(val) => info!("Task 1 completed successfully with result: {:?}", val),
                              Err(e) => break,
                        },

                        result = r_task =>match result {
                              Ok(val) => info!("Task 2 completed successfully with result: {:?}", val),
                              Err(e) => info!("task 2 error {:?}", e),
                        },
                }
            }

            Err(_) => {
                if retry_count < retry_max {
                    retry_count += 1;
                }

                let delay = (2_u64).pow(retry_count);
                println!("Failed to connect, retrying in {} seconds", delay);
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }
        }
    }
}

async fn get_worker_status() -> Result<String, Box<dyn std::error::Error>> {
    tracing::info!("Sending status request ");
    let client = reqwest::Client::new();
    let opml_server_url = format!("{}/api/v1/status", "http://127.0.0.1:1234");
    tracing::info!("{:?}", opml_server_url);

    let response = client.get(opml_server_url).send().await?;
    tracing::info!("{:?}", response);


    if response.status().is_success() {
    let body = response.text().await?;
    let parsed: Value = serde_json::from_str(&body).unwrap();
    tracing::info!("body {:?}", parsed);
    if let Some(msg) = parsed.get("msg") {
        info!("msg = {:?}", msg);
          return      Ok(serde_json::from_value::<String>(msg.clone()).unwrap());
    }
        Ok("0".into())

    } else {
        Err(format!("Tee server responded with status: {}", response.status()).into())
    }
}

