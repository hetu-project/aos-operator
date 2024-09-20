use crate::operator::OperatorArc;
use crate::queue::msg_queue::{MessageQueue, RedisMessage, RedisStreamPool};
use alloy::primitives::Address;
use alloy_wrapper::contracts::vrf_range;
use hex::FromHex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;
use verify_hub::opml::{
    handler::opml_question_handler,
    model::{OpmlAnswer, OpmlRequest},
};
use verify_hub::server::server::SharedState;
use verify_hub::tee::{
    handler::tee_question_handler,
    model::{AnswerReq, Params, QuestionReq},
};
use websocket::{connect, ReceiveMessage, WebsocketConfig, WebsocketSender};

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectRequest(pub Vec<ConnectParam>);

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectParam {
    pub operator: String,
    pub hash: String,
    pub signature: String,
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

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct JobResultRequest(pub Vec<JobResultParam>);

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct JobResultParam {
    pub user: String,
    pub job_id: String,
    pub result: String,
    pub tag: String,
    pub clock: HashMap<String, String>,
    pub operator: String,
    pub signature: String,
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
    let _res = sender
        .request_timeout(
            String::from_str("job_result").unwrap(),
            msg,
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();

    match serde_json::from_value(_res).unwrap() {
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
        req_id: "".to_owned(),
        callback: callback,
    };

    let qest = opml_question_handler(state, opml_request).await.unwrap();
    let worker_response: WorkerResponse = serde_json::from_value(qest).unwrap();
    let worker_result: OpmlAnswer = serde_json::from_value(worker_response.result).unwrap();

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
        //TODO
        operator: String::from_str("operator1").unwrap(),
        signature: String::from_str("signature").unwrap(),
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
    //TODO Mock
    let tee_request = QuestionReq {
        message: prompt,
        message_id: "1".to_owned(),
        conversation_id: "1".to_owned(),
        model: model,
        params: Params {
            temperature: temperature as f32,
            top_p: top_p as f32,
            max_tokens: max_tokens as u16,
        },
        callback_url: "http://127.0.0.1:21001/api/tee_callback".to_owned(),
    };

    let qest = tee_question_handler(state, tee_request).await.unwrap();
    let worker_response: WorkerResponse = serde_json::from_value(qest).unwrap();
    let worker_result: AnswerReq = serde_json::from_value(worker_response.result).unwrap();

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
        operator: String::from_str("operator1").unwrap(),
        signature: String::from_str("signature").unwrap(),
    }])
}

async fn do_job(id: String, msg: DispatchJobRequest, tx: WebsocketSender, op: OperatorArc) {
    let config = op.lock().await.config.clone();
    let contract = op.lock().await.vrf_range_contract.clone();
    let vrf_key = op.lock().await.vrf_key.clone();
    let state = op.lock().await.hub_state.clone();

    let bytes = <[u8; 20]>::from_hex(&config.node.node_id[2..]).unwrap_or_default();
    let addr: Address = Address::new(bytes);
    let (start, end) = vrf_range::get_range_by_address2(contract, addr)
        .await
        .unwrap();

    //TODO loop process
    let params = &msg.0[0];
    let job_id = params.job_id.clone();
    info!("dispatch params:{params:?}");

    let tag = params.tag.clone();
    let user = params.user.clone();
    //TODO use contract MAX
    let max_range = 10000.0;
    //adjust acording to tag and position
    let _range = match params.tag.as_str() {
        "suspicion" => (
            ((start as f64) - max_range * 0.03) as u64,
            ((end as f64) + max_range * 0.03) as u64,
        ),
        "malicious" => match params.position.as_str() {
            "before" => (
                ((start as f64) - max_range * 0.1) as u64,
                ((end as f64) + max_range * 0.1) as u64,
            ),
            "after" => (
                ((start as f64) - max_range * 0.03) as u64,
                ((end as f64) + max_range * 0.03) as u64,
            ),
            &_ => panic!("no way"),
        },
        _ => (start, end),
    };

    let vrf_threshold = end - start;
    let vrf_precision = config.chain.vrf_sort_precision as usize;
    let vrf_prompt_hash = params.job.prompt.clone();

    let _res = vrf_key.run_vrf(vrf_prompt_hash, vrf_precision, vrf_threshold);
    info!("vrf={:?}", _res);

    let callback = config.net.callback_url.clone();
    let mut res: JobResultRequest = Default::default();
    match params.job.tag.as_str() {
        "opml" => {
            res = do_opml_job(
                state.unwrap(),
                "llama-7b".to_owned(),
                params.job.prompt.clone(),
                user,
                job_id,
                tag,
                callback,
                &params.clock,
            )
            .await
        }
        "tee" => {
            res = do_tee_job(
                state.unwrap(),
                "llama-7b".to_owned(),
                params.job.prompt.clone(),
                user,
                job_id,
                tag,
                params.job.params.temperature,
                params.job.params.top_p,
                params.job.params.max_tokens,
                &params.clock,
            )
            .await
        }
        _ => panic!("no way"),
    }

    job_result(&tx, serde_json::to_value(&res).unwrap()).await;
}

async fn handle_dispatchjobs(
    operator: OperatorArc,
    sender: WebsocketSender,
    mut queue: RedisStreamPool,
) {
    loop {
        let msgs = queue.consume("opml").await.unwrap();

        for (_k, m) in msgs.iter().enumerate() {
            let (id, msg): (String, Value) = serde_json::from_str(m.data.as_str()).unwrap();
            info!("Consumed message: id={:?}, data={:?}", id, msg);

            let job: DispatchJobRequest = serde_json::from_value(msg).unwrap();

            let op = operator.clone();

            let sender_clone = sender.clone();
            do_job(id, job, sender_clone, op).await;

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
                                                let redis_msg = RedisMessage::new((id, p));
                                                info!("Product message: data={:?}", redis_msg);
                                                queue_clone
                                                    .produce("opml", &redis_msg)
                                                    .await
                                                    .unwrap();
                                            }
                                            &_ => panic!("no way"),
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
                s_task.await.unwrap();
                r_task.await.unwrap();
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
