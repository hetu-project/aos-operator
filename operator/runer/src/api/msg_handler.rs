use super::{msg::*, vrf_key::VRFReply};
use tracing_futures::Instrument;
use std::thread;
use crate::{
    error::{OperatorError, OperatorResult},
    operator::OperatorArc,
    queue::msg_queue::{MessageQueue, RedisMessage, RedisStreamPool},
};
use alloy::{primitives::Address, signers::local::PrivateKeySigner};
use alloy_wrapper::contracts::vrf_range;
use ed25519_dalek::{Digest, Sha512};
use hex::FromHex;
use node_api::config::NodeConfig;
use serde_json::Value;
use signer::msg_signer::MessageVerify;
use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::Arc};
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use verify_hub::{
    error::VerifyHubError,
    opml::{
        handler::opml_question_handler,
        model::{OpmlAnswer, OpmlRequest},
    },
    zkml::{
        handler::zkml_question_handler,
        model::{ZkmlAnswer, ZkmlRequest},
    },
    server::server::SharedState,
    tee::{
        handler::tee_question_handler,
        model::{AnswerReq, OperatorReq, Params},
    },
};
use websocket::{connect, ReceiveMessage, WebsocketConfig, WebsocketSender};


/// Asynchronously connects to a dispatcher using the given WebSocket sender and node ID.
///
/// This function sends a connection request to the dispatcher via the WebSocket `sender`
/// and waits for a response within a 5-second timeout period. The request includes a
/// `ConnectRequest` containing the provided `node_id`, and empty `hash` and `signature` fields.
///
/// If the connection response is received successfully, the response code and message
/// are logged for further analysis.
///
/// # Arguments
///
/// * `sender` - A reference to the `WebsocketSender` used to send the connection request.
/// * `node_id` - The ID of the operator node to connect.
///
/// # Returns
///
/// * `OperatorResult<()>` - Returns `Ok(())` on successful connection, or an error if the request or response fails.
///
/// # Errors
///
/// This function will return an error if:
/// - The JSON serialization of the request fails.
/// - The WebSocket request times out or fails to send.
/// - The JSON deserialization of the response fails.
pub async fn connect_dispatcher(sender: &WebsocketSender, node: &NodeConfig) -> OperatorResult<()> {
    let node_id = node.node_id.as_str();
    let node_type = node.node_type.clone();
    let res = sender
        .request_timeout(
            "connect".to_string(),
            serde_json::json!(&ConnectRequest(vec![ConnectParam {
                operator: node_id.to_owned(),
                workers: vec![node_type],
                hash: "".to_owned(),
                signature: "".to_owned(),
            }])),
            std::time::Duration::from_secs(5),
        )
        .await?;

    match serde_json::from_value(res)? {
        WsResponse { code, message } => {
            tracing::info!("<--connect respond: code={:?}, message={:?}", code, message);
        }
    };

    Ok(())
}

/// Asynchronously sends a job result to the dispatcher using the given WebSocket sender.
///
/// This function sends a job result message to the dispatcher using the provided WebSocket
/// `sender` and waits for a response within a 50-second timeout period. The `msg` parameter
/// contains the job result information in JSON format (`Value`).
///
/// If the response is received successfully, the response code and message are logged for further analysis.
///
/// # Arguments
///
/// * `sender` - A reference to the `WebsocketSender` used to send the job result message.
/// * `msg` - A `serde_json::Value` containing the job result information to be sent.
///
/// # Returns
///
/// * `OperatorResult<()>` - Returns `Ok(())` on successful communication, or an error if the request or response fails.
///
/// # Errors
///
/// This function will return an error if:
/// - The WebSocket request times out or fails to send.
/// - The JSON deserialization of the response fails.
pub async fn job_result(sender: &WebsocketSender, msg: Value) -> OperatorResult<()> {
    let res = sender
        .request_timeout(
            "job_result".to_string(),
            msg,
            std::time::Duration::from_secs(50),
        )
        .await?;

    match serde_json::from_value(res)? {
        WsResponse { code, message } => {
            tracing::info!("<--JobResult: code={:?}, message={:?}", code, message);
        }
    };

    Ok(())
}

/// Asynchronously executes an OPML job using the given parameters and shared state.
///
/// This function interacts with an OPML worker to perform a job, checking the worker's status
/// in a loop until it becomes available or the retry limit is reached. If the worker becomes
/// available, it sends an OPML request and handles the response. The function returns a
/// `JobResultRequest` containing the result of the operation.
///
/// # Arguments
///
/// * `state` - Shared state used for handling the OPML question.
/// * `model` - The model to use for generating the answer.
/// * `prompt` - The prompt for the OPML worker.
/// * `user` - The user who initiated the job.
/// * `job_id` - The identifier for the job.
/// * `tag` - A tag associated with the job.
/// * `callback` - Callback URL to notify the result.
/// * `clock` - A reference to a clock represented as a `HashMap` of `String` pairs.
///
/// # Returns
///
/// * `OperatorResult<JobResultRequest>` - Returns a successful `JobResultRequest` if the operation
///   completes successfully, or an appropriate `OperatorError` if there is an error.
///
/// # Errors
///
/// This function will return an error if:
/// - It fails to retrieve the worker status.
/// - The retry count exceeds the limit (600 times).
/// - The OPML request handler fails, such as a timeout or verification error.
/// - The clock value is missing or a numeric error occurs while incrementing the clock value.
async fn do_opml_job(
    state: SharedState,
    model: String,
    prompt: String,
    user: String,
    job_id: String,
    tag: String,
    callback: String,
    clock: &HashMap<String, String>,
    vrf_result: VRFReply
) -> OperatorResult<JobResultRequest> {
    let mut retry_send_count = 0;
    loop {
        if retry_send_count >= 600 {
            return Err(OperatorError::OPTimeoutError(
                "opml question timeout".into(),
            ));
        }

        let status = match get_opml_worker_status().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    "get opml worker status error: {:?}, retry count{}",
                    e,
                    retry_send_count
                );
                retry_send_count += 1;
                sleep(Duration::from_millis(10000)).await;
                continue;
            }
        };

        tracing::info!(
            "opml worker status: {:?}, retry count:{}",
            status,
            retry_send_count
        );
        if status == "0" {
            break;
        }

        sleep(Duration::from_millis(10000)).await;
        retry_send_count += 1;
    }

    let opml_request = OpmlRequest {
        model,
        prompt,
        req_id: Uuid::new_v4().to_string(),
        callback,
    };

    let worker_result: OpmlAnswer = match opml_question_handler(state, opml_request).await {
        Ok(result) => result,
        Err(VerifyHubError::RequestTimeoutError) => {
            return Err(OperatorError::OPTimeoutError(
                "opml question timeout".into(),
            ));
        }
        Err(e) => return Err(OperatorError::OPVerifyHubError(e)),
    };

    let mut key_value_vec: Vec<(&String, &String)> = clock.iter().collect();
    let (clk_id, clk_val) = key_value_vec
        .pop()
        .map(|(id, val)| (id.as_str(), val))
        .ok_or(OperatorError::CustomError(
            "opml clock value missing".into(),
        ))?;

    Ok(JobResultRequest(vec![JobResultParam {
        user,
        job_id,
        tag,
        //result: worker_result.answer[..=worker_result.answer.rfind('.').unwrap()].to_string(),
        result: worker_result.answer,
        vrf: vrf_result,
        clock: HashMap::from([(
            clk_id.to_owned(),
            clk_val
                .parse::<u32>()?
                .checked_add(1)
                .ok_or(OperatorError::CustomError("checked_add error".into()))?
                .to_string(),
        )]),
        operator: "".to_owned(),
        signature: "".to_owned(),
    }]))
}

/// Asynchronously executes a TEE job using the given parameters and shared state.
///
/// This function prepares a TEE request with the specified model, prompt, and parameters,
/// sends it to the TEE worker, and handles the response. If the worker returns an answer,
/// it constructs a `JobResultRequest` with the response and returns it.
///
/// # Arguments
///
/// * `state` - Shared state used for handling the TEE question.
/// * `model` - The model to use for generating the answer.
/// * `prompt` - The prompt for the TEE worker.
/// * `user` - The user who initiated the job.
/// * `job_id` - The identifier for the job.
/// * `tag` - A tag associated with the job.
/// * `temperature` - Temperature parameter for controlling randomness in the response.
/// * `top_p` - Top-p parameter for controlling response diversity.
/// * `max_tokens` - Maximum number of tokens allowed in the response.
/// * `clock` - A reference to a clock represented as a `HashMap` of `String` pairs.
///
/// # Returns
///
/// * `OperatorResult<JobResultRequest>` - Returns a successful `JobResultRequest` if the operation
///   completes successfully, or an appropriate `OperatorError` if there is an error.
///
/// # Errors
///
/// This function will return an error if:
/// - The TEE request handler fails due to a timeout or verification error.
/// - The clock value is missing or a numeric error occurs while incrementing the clock value.
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
    vrf_result: VRFReply
) -> OperatorResult<JobResultRequest> {
    let mut retry_send_count = 0;
    loop {
        if retry_send_count >= 600 {
            return Err(OperatorError::OPTimeoutError(
                "opml question timeout".into(),
            ));
        }

        let status = match get_tee_worker_status().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(
                    "get tee worker status error: {:?}, retry count{}",
                    e,
                    retry_send_count
                );
                retry_send_count += 1;
                sleep(Duration::from_millis(10000)).await;
                continue;
            }
        };

        tracing::info!(
            "tee worker status: {:?}, retry count:{}",
            status,
            retry_send_count
        );
        if status > 0 {
            break;
        }

        sleep(Duration::from_millis(10000)).await;
        retry_send_count += 1;
    }

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

    let worker_result: AnswerReq = match tee_question_handler(state, tee_request).await {
        Ok(result) => result,
        Err(VerifyHubError::RequestTimeoutError) => {
            return Err(OperatorError::OPTimeoutError("tee question timeout".into()))
        }
        Err(e) => return Err(OperatorError::OPVerifyHubError(e)),
    };

    let mut key_value_vec: Vec<(&String, &String)> = clock.iter().collect();
    let (clk_id, clk_val) = key_value_vec
        .pop()
        .map(|(id, val)| (id.as_str(), val))
        .ok_or(OperatorError::CustomError("tee clock value missing".into()))?;

    Ok(JobResultRequest(vec![JobResultParam {
        user,
        job_id,
        tag,
        result: worker_result.answer,
        vrf: vrf_result,
        clock: HashMap::from([(
            clk_id.to_owned(),
            clk_val
                .parse::<u32>()?
                .checked_add(1)
                .ok_or(OperatorError::CustomError("checked_add error".into()))?
                .to_string(),
        )]),
        operator: "".to_owned(),
        signature: "".to_owned(),
    }]))
}

async fn do_zkml_job(
    state: SharedState,
    model: String,
    proof_path: String,
    user: String,
    job_id: String,
    tag: String,
    callback: String,
    clock: &HashMap<String, String>,
    vrf_result: VRFReply
) -> OperatorResult<JobResultRequest> {

    let zkml_request = ZkmlRequest {
        model,
        req_id: Uuid::new_v4().to_string(),
        callback,
        proof_path
    };

    let worker_result: ZkmlAnswer = match zkml_question_handler(state, zkml_request).await {
        Ok(result) => result,
        Err(VerifyHubError::RequestTimeoutError) => {
            return Err(OperatorError::OPTimeoutError(
                "zkml question timeout".into(),
            ));
        }
        Err(e) => return Err(OperatorError::OPVerifyHubError(e)),
    };

    tracing::info!("{:?}", worker_result);

    let mut key_value_vec: Vec<(&String, &String)> = clock.iter().collect();
    let (clk_id, clk_val) = key_value_vec
        .pop()
        .map(|(id, val)| (id.as_str(), val))
        .ok_or(OperatorError::CustomError(
            "opml clock value missing".into(),
        ))?;

    Ok(JobResultRequest(vec![JobResultParam {
        user,
        job_id,
        tag,
        //result: worker_result.answer[..=worker_result.answer.rfind('.').unwrap()].to_string(),
        result: worker_result.result, //worker_result.answer,
        vrf: vrf_result,
        clock: HashMap::from([(
            clk_id.to_owned(),
            clk_val
                .parse::<u32>()?
                .checked_add(1)
                .ok_or(OperatorError::CustomError("checked_add error".into()))?
                .to_string(),
        )]),
        operator: "".to_owned(),
        signature: "".to_owned(),
    }]))
}

/// Asynchronously computes a Verifiable Random Function (VRF) reply based on given parameters.
///
/// This function locks the operator state to obtain configuration, contract, and VRF key information.
/// It determines the VRF range for a given address, adjusts the range according to the provided tag and
/// position, and calculates a VRF reply using the prompt hash and specified precision. The computed VRF
/// is then returned as a `VRFReply`.
///
/// # Arguments
///
/// * `op` - A shared reference to the operator state (`OperatorArc`) for accessing configuration and VRF information.
/// * `tag` - A tag indicating the type of operation (e.g., "suspicion", "malicious").
/// * `position` - The position context ("before", "after") used for adjusting the VRF range.
/// * `prompt` - The prompt for generating the VRF hash.
///
/// # Returns
///
/// * `OperatorResult<VRFReply>` - Returns a successful `VRFReply` containing the VRF output, or an appropriate
///   `OperatorError` if there is an error.
///
/// # Errors
///
/// This function will return an error if:
/// - The address conversion from the node ID fails.
/// - The VRF range retrieval fails.
/// - The tag and position combination is unsupported.
async fn compute_vrf(
    op: OperatorArc,
    tag: String,
    position: String,
    prompt: String,
) -> OperatorResult<VRFReply> {
    let config = op.lock().await.config.clone();
    let contract = op.lock().await.vrf_range_contract.clone();
    let vrf_key = op.lock().await.vrf_key.clone();
    let addr: Address = Address::new(<[u8; 20]>::from_hex(&config.node.node_id[2..])?);

    let (start, end) = vrf_range::get_range_by_address(contract.clone(), addr)
        .await
        .map(|(x, y)| (x as f64, y as f64))?;

    tracing::info!("vrf range: addr-{addr}, start-{start:?}, end-{end:?}");

    //TODO Mock
    //let max_range = vrf_range::get_max_range(contract).await? as f64;
    let max_range = vrf_range::get_max_range(contract)
        .await
        .or_else(|_e| Ok::<u64, eyre::Report>(0xffffffff))
        .unwrap() as f64;

    //adjust according to tag and position
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
            &_ => return Err(OperatorError::OPUnsupportedTagError(position)),
        },
        _ => (start as u64, end as u64),
    };

    tracing::info!(
        "after adjusting -{max_range:?},star-{:?} end-{:?}",
        range.0,
        range.1
    );

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

    Ok(vrf_key.run_vrf(vrf_prompt_hash, vrf_precision, vrf_threshold, max_range as u64)?)
}

/// Asynchronously performs a job dispatch process using the given parameters.
///
/// This function processes the job request by determining the job parameters, computing the
/// Verifiable Random Function (VRF) value, and invoking specific job handlers (`do_opml_job`
/// or `do_tee_job`) depending on the job type. It also updates the storage with the results
/// and sends the job result over a WebSocket connection.
///
/// # Arguments
///
/// * `msg` - The job request message containing job parameters.
/// * `sender` - A WebSocket sender used to send job results back to the requester.
/// * `op` - A shared reference to the operator state (`OperatorArc`) for accessing configuration and state.
///
/// # Returns
///
/// * `OperatorResult<()>` - Returns an empty result if the operation completes successfully,
///   or an appropriate `OperatorError` if there is an error.
///
/// # Errors
///
/// This function will return an error if:
/// - The operator state cannot be locked or cloned.
/// - The VRF computation fails.
/// - The clock value is missing or a numeric error occurs while incrementing the clock value.
/// - The job tag is unsupported.
/// - The job storage update fails.
/// - An error occurs when sending the job result over the WebSocket connection.
async fn do_job(
    msg: DispatchJobRequest,
    sender: WebsocketSender,
    op: OperatorArc,
) -> OperatorResult<()> {
    let config = op.lock().await.config.clone();
    let node_type = config.node.node_type.clone();
    let state = op
        .lock()
        .await
        .hub_state
        .clone()
        .ok_or(OperatorError::CustomError("SharedState clone error".into()))?;
    let storage = op.lock().await.storage.clone();

    //TODO for params in &msg.0.iter() {}
    let params = &msg.0[0];
    let job_id = params.job_id.clone();
    tracing::info!("dispatch params:{:?}", params);

    let position = params.position.clone();
    let user = params.user.clone();

    //compute vrf
    let vrf_result =
        compute_vrf(op, params.tag.clone(), position, params.job.prompt.clone()).await?;
    tracing::info!("vrf result: {:?}", vrf_result);

    let mut res = if msg.0[0].job.tag != node_type {
        tracing::warn!("unsupported operator : {:?}", msg.0[0].job.tag);

        let mut key_value_vec: Vec<(&String, &String)> = params.clock.iter().collect();
        let (clk_id, clk_val) = key_value_vec
            .pop()
            .map(|(id, val)| (id.as_str(), val))
            .ok_or(OperatorError::CustomError("Clock value missing".into()))?;

        JobResultRequest(vec![JobResultParam {
            user,
            job_id,
            tag: params.tag.clone(),
            result: format!(
                "unsupported worker type, need {}, got {}",
                node_type, msg.0[0].job.tag
            ),
            vrf: vrf_result,
            clock: HashMap::from([(
                clk_id.to_owned(),
                clk_val
                    .parse::<u32>()?
                    .checked_add(1)
                    .ok_or(OperatorError::CustomError("checked_add error".into()))?
                    .to_string(),
            )]),
            operator: "".to_owned(),
            signature: "".to_owned(),
        }])
    } else if vrf_result.selected == false {
        let mut key_value_vec: Vec<(&String, &String)> = params.clock.iter().collect();
        let (clk_id, clk_val) = key_value_vec
            .pop()
            .map(|(id, val)| (id.as_str(), val))
            .ok_or(OperatorError::CustomError("Clock value missing".into()))?;

        JobResultRequest(vec![JobResultParam {
            user,
            job_id,
            tag: params.tag.clone(),
            result: "".to_owned(),
            vrf: vrf_result,
            clock: HashMap::from([(
                clk_id.to_owned(),
                clk_val
                    .parse::<u32>()?
                    .checked_add(1)
                    .ok_or(OperatorError::CustomError("checked_add error".into()))?
                    .to_string(),
            )]),
            operator: "".to_owned(),
            signature: "".to_owned(),
        }])
    } else {
        match params.job.tag.as_str() {
            "opml" => {
                do_opml_job(
                    state,
                    params.job.model.to_owned(),
                    params.job.prompt.clone(),
                    user,
                    job_id,
                    params.tag.clone(),
                    config.net.callback_url.clone(),
                    &params.clock,
                    vrf_result
                )
                .await?
            }
            "tee" => {
                do_tee_job(
                    state,
                    params.job.model.to_owned(),
                    params.job.prompt.clone(),
                    user,
                    job_id,
                    params.tag.clone(),
                    params.job.params.temperature.unwrap(),
                    params.job.params.top_p.unwrap(),
                    params.job.params.max_tokens.unwrap(),
                    &params.clock,
                    vrf_result
                )
                .await?
            }
            "zkml" => {
                do_zkml_job(
                    state,
                    params.job.model.to_owned(),
                    params.job.params.proof_path.as_ref().unwrap().to_string(),
                    user,
                    job_id,
                    params.tag.clone(),
                    config.net.callback_url.clone(),
                    &params.clock,
                    vrf_result
                )
                .await?
            }
            _ => return Err(OperatorError::OPUnsupportedTagError(params.job.tag.clone())),
        }
    };

    storage
        .sinker_job(
            res.0[0].job_id.clone(),
            res.0[0].user.clone(),
            res.0[0].result.clone(),
            res.0[0].tag.clone(),
            serde_json::to_string(&res.0[0].clock)?,
        )
        .await;

    tracing::info!("-->job_result: {:?}", &res);
    if let Err(e) = job_result(&sender, serde_json::json!(&res)).await {
        tracing::error!("send job_result got error: {:?}", e);
    }

    Ok(())
}

/// Handles the dispatching of jobs by consuming messages from a Redis queue.
///
/// This function runs in a loop, continuously consuming messages from a Redis queue.
/// Each message is deserialized and transformed into a `DispatchJobRequest`. If the
/// message cannot be parsed or if the job tag does not match the node type, appropriate
/// logs are generated and the message is acknowledged.
///
/// The function performs the following steps for each message:
/// 1. Consume a message from the Redis queue.
/// 2. Deserialize the message to a JSON value and log any errors that occur.
/// 3. Convert the deserialized message into a `DispatchJobRequest`.
/// 4. Verify if the job tag matches the node type. If not, log a warning.
/// 5. If the tag is supported, call `do_job()` asynchronously to process the job.
/// 6. Handle any errors that occur during job processing by logging them.
/// 7. Acknowledge the processed message in the Redis queue.
///
/// # Parameters
/// - `operator`: A shared reference (`Arc<Mutex<Operator>`) to the operator instance, used to access configurations.
/// - `sender`: A WebSocket sender used to send responses for the processed jobs.
/// - `queue`: A `RedisStreamPool` instance representing the Redis queue from which to consume messages.
async fn handle_dispatchjobs(
    operator: OperatorArc,
    sender: WebsocketSender,
    queue: RedisStreamPool,
) {
    let config = operator.lock().await.config.clone();
    let node_type = config.node.node_type.clone();

    let queue_topic = &config.queue.topic.as_str();
    loop {
        match queue.consume(queue_topic).await {
            Ok(msgs) => {
                for (_k, m) in msgs.iter().enumerate() {
                    //Deserialize data
                    let (id, msg): (String, Value) = match serde_json::from_str(m.data.as_str()) {
                        Ok(parsed) => parsed,
                        Err(e) => {
                            tracing::error!("Failed to parse message: {}, error: {:?}", m.data, e);
                            if let Err(e) = queue.acknowledge(queue_topic, &m.id).await {
                                tracing::error!(
                                    "Failed to acknowledge message: {}, error: {:?}",
                                    m.id,
                                    e
                                );
                            }
                            continue;
                        }
                    };

                    // convert msg into DispatchJobRequest
                    tracing::info!("Consumed message: id={:?}, data={:?}", id, msg);
                    let job: DispatchJobRequest = match serde_json::from_value(msg) {
                        Ok(parsed_job) => parsed_job,
                        Err(e) => {
                            tracing::error!("Failed to parse job, error: {:?}", e);
                            if let Err(e) = queue.acknowledge(queue_topic, &m.id).await {
                                tracing::error!(
                                    "Failed to acknowledge message: {}, error: {:?}",
                                    m.id,
                                    e
                                );
                            }
                            continue;
                        }
                    };

                    let op = operator.clone();
                    let sender_clone = sender.clone();

                    if let Err(e) = do_job(job, sender_clone, op).await {
                        match e {
                            OperatorError::CustomError(e) => {
                                tracing::error!("Failed to finish Job: {}, error: {:?}", m.id, e)
                            }
                            OperatorError::OPIoError(e) => {
                                tracing::error!("Failed to finish Job: {}, error: {:?}", m.id, e)
                            }
                            OperatorError::OPUnsupportedTagError(e) => {
                                tracing::error!("Failed to finish Job: {}, error: {:?}", m.id, e)
                            }
                            OperatorError::OPNewVrfRangeContractError(e) => {
                                tracing::error!("Failed to finish Job: {}, error: {:?}", m.id, e)
                            }

                            OperatorError::OPVerifyHubError(e) => {
                                tracing::error!("Failed to finish Job: {}, error: {:?}", m.id, e)
                            }
                            _e => {
                                tracing::error!("Failed to finish Job: {}, error: {}", m.id, _e)
                            }
                        }
                    }

                    // ack
                    if let Err(e) = queue.acknowledge(queue_topic, &m.id).await {
                        tracing::error!("Failed to acknowledge message: {}, error: {:?}", m.id, e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to consume messages from queue: {:?}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// Handles the main connection loop for establishing and maintaining a WebSocket connection.
///
/// This function is responsible for initializing a Redis queue, parsing the private key,
/// establishing a WebSocket connection, and managing message reception and dispatching tasks.
/// If a connection fails, the function attempts to reconnect with exponential backoff up to a
/// maximum number of retries.
///
/// The function performs the following steps:
/// 1. Initializes a Redis stream pool for the job queue.
/// 2. Parses the private key from a hexadecimal string and creates a signer.
/// 3. Parses the WebSocket address and attempts to establish a connection using the signer.
/// 4. If the connection is successful:
///    - Spawns a task to handle incoming messages from the WebSocket (`r_task`).
///    - Spawns a task to handle job dispatching (`s_task`).
///    - Uses `tokio::select!` to run both tasks concurrently and handle their completion or failure.
/// 5. If the connection fails, retries the connection using exponential backoff until the retry limit is reached.
///
/// # Parameters
/// - `op`: A shared reference (`Arc<Mutex<Operator>`) to the operator instance, used to access configuration and maintain the state.
///
/// # Returns
/// - `OperatorResult<()>`: Returns `Ok(())` if the loop completes successfully, or an error variant if any operation fails.
pub async fn handle_connection(op: OperatorArc) -> OperatorResult<()> {
    let mut id:i32 = 0;

    let config = op.lock().await.config.clone();
    let queue = match RedisStreamPool::new(&config.queue.queue_url).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("redis queue init error: {:?}", e);
            return Err(OperatorError::CustomError("redis init error".into()));
        }
    };
    let mut retry_count = 0;
    let retry_max = 8;

    loop {
        id+=1;
        let task_span = tracing::span!(tracing::Level::INFO, "task", task_id = id);
        let _enter =  task_span.enter();
        let key_bytes = match <[u8; 32]>::from_hex(&config.node.signer_key) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("key_bytes from hex error: {:?}", e);
                return Err(OperatorError::CustomError(format!(
                    "key_bytes from hex error: {:?}",
                    e
                )));
            }
        };
        let secret_key = match PrivateKeySigner::from_slice(&key_bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("private key from slice error: {:?}", e);
                return Err(OperatorError::CustomError(
                    "32 bytes, within curve order".into(),
                ));
            }
        };
        let signer = MessageVerify(secret_key);
        let socket = match SocketAddr::from_str(&config.net.dispatcher_url) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Websocket address parse error: {:?}", e);
                return Err(OperatorError::CustomError(
                    "Websocket address parse error".into(),
                ));
            }
        };

        match connect(Arc::new(WebsocketConfig::default()), socket, signer).await {
            Ok((sender, mut receiver)) => {
                retry_count = 0;
                let queue_clone = queue.clone();
                let topic = config.queue.topic.clone();

                let mut r_task = tokio::task::spawn(async move {
                    loop {
                        match receiver.recv().await {
                            Ok(msg) => {
                                tracing::info!("<--job request: msg={:?}", msg);

                                match msg {
                                    ReceiveMessage::Signal(s) => {
                                        tracing::warn!("signal message: {s:?}")
                                    }
                                    ReceiveMessage::Request(id, m, p, r) => {
                                        match m.as_str() {
                                            "dispatch_job" => {
                                                sleep(Duration::from_millis(10)).await;
                                                add_queue_req(&queue_clone, &topic, id, p).await;
                                            }
                                            method @ &_ => {
                                                tracing::warn!(
                                                    "Unexpected method value: {:?}",
                                                    method
                                                )
                                            }
                                        };

                                        let rep = serde_json::json!(WsResponse {
                                            code: 200,
                                            message: "success".to_string(),
                                        });

                                        if let Err(e) = r.respond(rep).await {
                                            tracing::error!("Failed to send message: {:?}", e);
                                            return;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                panic!("websocket error {:?}, need reconnect", e);
                            }
                        }
                    }
                }.instrument(task_span.clone()));

                if let Err(e) = connect_dispatcher(&sender, &config.node).await {
                    tracing::error!("connect server got error: {:?}", e);
                    break;
                }

                let _operator_clone = op.clone();
                let queue_cl = queue.clone();
                let mut s_task = tokio::task::spawn(async move {
                    handle_dispatchjobs(_operator_clone, sender, queue_cl).await;
                }.instrument(task_span.clone()));

                tokio::select! {
                        result = &mut s_task =>match result {
                              Ok(val) => tracing::info!("Task 1 completed successfully with result: {:?}", val),
                              Err(e) => break,
                        },

                        result = &mut r_task =>match result {
                              Ok(val) => tracing::info!("Task 2 completed successfully with result: {:?}", val),
                              Err(e) => {
                                  tracing::info!("task 2 error {:?}", e);
                                  s_task.abort();
                              }
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

    Ok(())
}

async fn get_opml_worker_status() -> OperatorResult<String> {
    tracing::info!("Sending opml status request");
    let client = reqwest::Client::new();
    let opml_server_url = format!("{}/api/v1/status", "http://127.0.0.1:1234");
    tracing::info!("{:?}", opml_server_url);

    let response = client.get(opml_server_url).send().await?;
    tracing::info!("{:?}", response);

    if response.status().is_success() {
        let body = response.text().await?;
        let parsed: Value = serde_json::from_str(&body)?;
        tracing::info!("body {:?}", parsed);
        let msg = parsed
            .get("msg")
            .ok_or(OperatorError::CustomError("parse msg error".into()))?;
        tracing::info!("msg = {:?}", msg);
        return Ok(serde_json::from_value::<String>(msg.clone())?);
    } else {
        Err(OperatorError::CustomError(
            format!("Opml server responded with status: {}", response.status()).into(),
        ))
    }
}

async fn get_tee_worker_status() -> OperatorResult<u32> {
    tracing::info!("Sending tee status request");
    let client = reqwest::Client::new();
    let tee_server_url = format!("{}/api/v1/status", "http://127.0.0.1:3000");
    tracing::info!("{:?}", tee_server_url);

    let response = client.get(tee_server_url).send().await?;
    tracing::info!("{:?}", response);

    if response.status().is_success() {
        let body = response.text().await?;
        let parsed: Value = serde_json::from_str(&body)?;
        tracing::info!("body {:?}", parsed);
        let msg = parsed
            .get("remain_task")
            .ok_or(OperatorError::CustomError("parse msg error".into()))?;
        tracing::info!("remain_task = {:?}", msg);
        return Ok(serde_json::from_value::<u32>(msg.clone())?);
    } else {
        Err(OperatorError::CustomError(
            format!("Tee server responded with status: {}", response.status()).into(),
        ))
    }
}

async fn add_queue_req(queue_clone: &RedisStreamPool, topic: &str, id: String, p: Value) -> OperatorResult<()> {
    let redis_msg = match RedisMessage::new((id, p)) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("redis message new error: {:?}", e);
            return Err(OperatorError::CustomError("redis msg new error".into()));
        }
    };
    tracing::info!("Product message: data={:?}", redis_msg);
    if let Err(e) = queue_clone.produce(topic, &redis_msg).await {
        tracing::error!("redis queue produce error: {:?}", e);
        return Err(OperatorError::CustomError(
            "redis queue produce error".into(),
        ));
    }

    Ok(())
}
