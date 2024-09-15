use crate::operator::Operator;
use crate::operator::OperatorArc;
use alloy_wrapper::contracts::vrf_range;
use node_api::error::OperatorAPIResult;
use num_bigint::BigUint;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;
use vrf::{
    ecvrf::{Output, VRFPrivateKey, VRFPublicKey, OUTPUT_LENGTH},
    sample::Sampler as VRFSampler,
};
use websocket::{ReceiveMessage, WebsocketReceiver, WebsocketSender, WireMessage};

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct VRFReply {
    pub selected: bool,
    pub vrf_prompt_hash: String,
    pub vrf_random_value: String,
    pub vrf_verify_pubkey: String,
    pub vrf_proof: String,
}

pub struct VRFPrivKey(pub VRFPrivateKey);

impl VRFPrivKey {
    pub fn new() -> Self {
        let private_key = VRFPrivateKey::generate_keypair(&mut OsRng);
        VRFPrivKey(private_key)
    }

    pub fn run_vrf(
        &self,
        vrf_prompt_hash: String,
        vrf_precision: usize,
        vrf_threshold: u64,
    ) -> Result<VRFReply, std::io::Error> {
        let public_key: VRFPublicKey = (&self.0).into();
        let proof: vrf::ecvrf::Proof = self.0.prove(vrf_prompt_hash.as_bytes());
        let output: Output = (&proof).into();
        let start = OUTPUT_LENGTH * 2 - vrf_precision;
        let end = OUTPUT_LENGTH * 2;
        let random_num = hex::encode(output.to_bytes());
        let random_str = &random_num[start..end];
        let vrf_sampler = VRFSampler::new(vrf_precision * 4);
        let random_bigint = vrf_sampler.hex_to_biguint(random_str);
        let selected = vrf_sampler.meets_threshold(&random_bigint, &BigUint::from(vrf_threshold));
        Ok(VRFReply {
            selected,
            vrf_prompt_hash: vrf_prompt_hash,
            vrf_random_value: random_num,
            vrf_verify_pubkey: hex::encode(public_key.as_bytes()),
            vrf_proof: hex::encode(proof.to_bytes()),
        })
    }
}
