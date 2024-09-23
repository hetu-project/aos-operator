use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// Represents a connect request containing a vector of ConnectParam
// operator -> dispatcher request
// {
// 	"id": "",
// 	"method": "connect",
// 	"params": [{
// 			"operator": "",
// 			"hash": "",
// 			"signature": "", }], "address": "",
// 	"hash": "",
// 	"signature": "",
// }
//
// ConnectRequest as params wrapped in WebSocket Request
#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectRequest(pub Vec<ConnectParam>);

// Represents the parameters for a connect request
#[derive(Serialize, Deserialize, Debug)]
pub struct ConnectParam {
    pub operator: String,
    pub hash: String,
    pub signature: String,
}

// Represents a dispatch job request containing a vector of DispatchJobParam
// dispatcher -> operator request
// {
// 	"id": "1122",
// 	"method": "dispatch_job",
// 	"params": [{
// 		"user": "", // user entry id
// 		"seed": "",
// 		"signature": "",
// 		"tag": "suspicion", // malicious suspicion
// 		"job_id": "",
//      "clock": {
// 				"1": "2"
// 		},
// 		"position": "", // before after
// 		"job": {
// 				"model": "",
// 				"prompt": "",
// 				"tag": "opml" //"opml" or "tee" or zk
// 				"params": {
// 					"temperature": 1.0,
// 					"top_p": 0.5,
// 					"max_tokens": 1024
// 				}
// 		}
// 	}],
// 	"address": "",
// 	"hash": "",
// 	"signature": "",
// }
// DispatchJobRequest as "params" wrapped in WebSocket Request
#[derive(Serialize, Deserialize, Debug)]
pub struct DispatchJobRequest(pub Vec<DispatchJobParam>);

// Represents the parameters for a dispatch job request
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

// Represents a job
#[derive(Serialize, Deserialize, Debug)]
pub struct Job {
    pub model: String,
    pub prompt: String,
    pub tag: String,
    pub params: JobParams,
}

// Represents the parameters for a job
#[derive(Serialize, Deserialize, Debug)]
pub struct JobParams {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u64,
}

// Represents a job result request containing a vector of JobResultParam
// operator -> dispatcher request
//{
//	"id": "",
//	"method": "job_result",
//	"params": [{
//			"user": "",
//			"job_id": "",
//			"tag": "suspicion",
//			"result": "",
//			"clock": {
//				"1": "2"
//			},
//			"operator": "",
//			"signature": "",
//	}],
//	"address": "",
//	"hash": "",
//	"signature": "",
//}
// JobResultRequest as "params" wrapped in WebSocket Request
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct JobResultRequest(pub Vec<JobResultParam>);

// Represents the parameters for a job result request
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

// Represents a WebSocket response
//	"result": {
//		"code": 200，
//		"message": "success"
//	},
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
