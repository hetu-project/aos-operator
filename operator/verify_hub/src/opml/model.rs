use crate::tee::model::{deserialize_naive_datetime, serialize_naive_datetime};
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpmlAnswer {
    pub req_id: String,
    pub node_id: String,
    pub model: String,
    pub prompt: String,
    pub answer: String,
    pub state_root: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpmlAnswerResponse {
    pub code: u16,
    pub result: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpmlRequest {
    pub model: String,
    pub prompt: String,
    pub req_id: String,
    pub callback: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpmlResponse {
    pub code: u16,
    pub msg: String,
    pub data: OpmlResponseData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpmlResponseData {
    pub node_id: String,
    pub req_id: String,
}
