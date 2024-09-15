use serde::{Deserialize, Serialize};
use serde_json::Result;

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectRequest {
    pub operator: String,
    pub hash: String,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectResponse {
    pub code: u16,
    pub message: String,
}

fn test_msg() -> Result<()> {
    let request = ConnectRequest {
        operator: "operator_address".to_string(),
        hash: "some_hash".to_string(),
        signature: "signature_data".to_string(),
    };

    let response = ConnectResponse {
        code: 200,
        message: "success".to_string(),
    };

    // 序列化为 Vec<u8>
    let serialized_request: Vec<u8> = serde_json::to_vec(&request)?;
    let serialized_response: Vec<u8> = serde_json::to_vec(&response)?;

    // 输出序列化结果
    println!("Serialized Request (Vec<u8>): {:?}", serialized_request);
    println!("Serialized Response (Vec<u8>): {:?}", serialized_response);

    // 反序列化回结构体
    let deserialized_request: ConnectRequest = serde_json::from_slice(&serialized_request)?;
    let deserialized_response: ConnectResponse = serde_json::from_slice(&serialized_response)?;

    // 输出反序列化结果
    println!("Deserialized Request: {:?}", deserialized_request);
    println!("Deserialized Response: {:?}", deserialized_response);

    Ok(())
}

pub fn main() {
    test_msg();
}
