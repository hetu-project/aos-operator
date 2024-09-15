use chrono::NaiveDateTime;
use diesel::associations::HasTable;
use diesel::prelude::*;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operator {
    pub worker_name: String,
    pub check_heart_beat: bool,
    pub worker_status: WorkerStatus,
    pub multimodal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatus {
    pub model_names: Vec<String>,
    pub speed: u16,
    pub queue_length: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorReq {
    pub request_id: String,
    pub node_id: String,
    pub model: String,
    pub prompt: String,
    pub prompt_hash: String,
    pub signature: String,
    pub params: Params,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Params {
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorResp {
    pub request_id: String,
    pub code: u16,
    pub msg: String,
    pub data: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerReq {
    pub request_id: String,
    pub node_id: String,
    pub model: String,
    pub prompt: String,
    pub answer: String,
    pub attestation: String,
    pub attest_signature: String,
    pub elapsed: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerResp {
    pub code: u16,
    pub result: String,
}

#[derive(Serialize, Deserialize)]
pub struct HashRequest {
    pub hash: String,
}

#[derive(Serialize)]
pub struct HashResponse {
    pub sig: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuestionReq {
    pub message: String,
    pub message_id: String,
    pub conversation_id: String,
    pub model: String,
    pub params: Params,
    pub callback_url: String,
}

#[derive(Serialize, Deserialize)]
pub struct QuestionResp {
    pub code: u16,
    pub result: QuestionResult,
}

#[derive(Serialize, Deserialize)]
pub struct QuestionResult {
    pub id: String,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterResp {
    pub code: u16,
    pub result: String,
}

#[derive(Serialize, Deserialize)]
pub struct HeartBeatResp {
    pub exist: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HeartBeatReq {
    pub worker_name: String,
    pub queue_length: u16,
}

pub fn serialize_naive_datetime<S>(date: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let s = date.format("%Y-%m-%d %H:%M:%S").to_string();
    serializer.serialize_str(&s)
}

pub fn deserialize_naive_datetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").map_err(serde::de::Error::custom)
}

pub async fn forward_answer_to_callback(
    ans: &AnswerReq,
    callback: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    // Create a payload to send to the callback URL
    let payload = serde_json::json!({
        "request_id": ans.request_id,
        "node_id": ans.node_id,
        "model": ans.model,
        "prompt": ans.prompt,
        "answer": ans.answer,
        "attestation": ans.attestation,
        "attest_signature": ans.attest_signature,
        "elapsed": ans.elapsed,
    });

    let url = Url::parse(&callback)?;

    // Send the POST request to the callback URL
    let response = client
        .post(url)
        .json(&payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    // Check if the request was successful
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("Failed to forward answer. Status: {}", response.status()).into())
    }
}
