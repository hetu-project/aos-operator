use super::msg::*;
use super::vrf_key::VRFReply;
use crate::operator::OperatorArc;
use crate::queue::msg_queue::{MessageQueue, RedisMessage, RedisStreamPool};
use alloy::primitives::Address;
use alloy::signers::local::yubihsm::setup::Report;
use alloy_wrapper::contracts::vrf_range;
use ed25519_dalek::{Digest, Sha512};
use hex::FromHex;
use serde_json::Value;
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

pub async fn connect_dispatcher(sender: &WebsocketSender) {
    //TODO Mock
    let res = sender
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
        result: worker_result.answer[..=worker_result.answer.rfind('.').unwrap()].to_string(),
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
    let max_range = vrf_range::get_max_range(contract)
        .await
        .or_else(|_e| Ok::<u64, Report>(10000))
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

    //for params in &msg.0.iter() {}
    let params = &msg.0[0];
    let job_id = params.job_id.clone();
    info!("dispatch params:{params:?}");

    let position = params.position.clone();
    let user = params.user.clone();

    let vrf_result = compute_vrf(op, params.tag.clone(), position, params.job.prompt.clone())
        .await
        .unwrap();
    if vrf_result.selected == false {
        //TODO return to dispatcher
    }

    let res = match params.job.tag.as_str() {
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
    };

    job_result(&sender, serde_json::to_value(&res).unwrap()).await;
}

async fn handle_dispatchjobs(
    operator: OperatorArc,
    sender: WebsocketSender,
    mut queue: RedisStreamPool,
) {
    loop {
        let msgs = queue.consume("opml").await.unwrap();

        for (_k, m) in msgs.iter().enumerate() {
            //TODO worker if free?
            //
            let (id, msg): (String, Value) = serde_json::from_str(m.data.as_str()).unwrap();
            info!("Consumed message: id={:?}, data={:?}", id, msg);

            let job: DispatchJobRequest = serde_json::from_value(msg).unwrap();
            let op = operator.clone();
            let sender_clone = sender.clone();

            do_job(job, sender_clone, op).await;

            //TODO save to db

            queue.acknowledge("opml", &m.id).await.unwrap();
        }
    }
}

pub async fn handle_connection(op: OperatorArc) {
    let config = op.lock().await.config.clone();
    let queue = RedisStreamPool::new(&config.queue.queue_url).await.unwrap();
    let mut retry_count = 0;

    loop {
        let socket = SocketAddr::from_str(&config.net.dispatcher_url).unwrap();
        match connect(Arc::new(WebsocketConfig::default()), socket).await {
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
                                break;
                            }
                        }
                    }
                });

                connect_dispatcher(&sender).await;

                let _operator_clone = op.clone();
                let queue_cl = queue.clone();
                let s_task = tokio::task::spawn(async move {
                    handle_dispatchjobs(_operator_clone, sender, queue_cl).await;
                });
                let results = futures::future::join_all(vec![s_task, r_task]).await;

                for result in results {
                    match result {
                        Ok(value) => println!("Task completed with value: {:?}", value),
                        Err(e) => eprintln!("Task failed: {:?}", e),
                    }
                }
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
