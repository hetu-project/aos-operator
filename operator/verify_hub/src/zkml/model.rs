use crate::tee::model::{deserialize_naive_datetime, serialize_naive_datetime};
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkmlAnswer {
    pub req_id: String,
    //pub node_id: String,
    //pub model: String,
    //pub prompt: String,
    //pub answer: String,
    //pub state_root: String,
    pub code: u16,
    pub result: String
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ZkmlAnswerResponse {
    pub code: u16,
    pub result: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ZkmlRequest {
    pub model: String,
    pub req_id: String,
    pub callback: String,
    pub proof_path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ZkmlResponse {
    pub code: u16,
    pub msg: String,
    pub data: ZkmlResponseData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ZkmlResponseData {
    pub node_id: String,
    pub req_id: String,
}
