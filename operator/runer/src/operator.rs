use crate::api::vrf_key::VRFPrivKey;
use crate::{node_factory::OperatorFactory, storage::Storage};
use alloy_primitives::B256;
use alloy_wrapper::contracts::vrf_range::OperatorRangeContract;
use node_api::config::OperatorConfig;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use verify_hub::server::server::SharedState;
use websocket::*;

pub struct Operator {
    pub config: Arc<OperatorConfig>,
    pub storage: Storage,
    pub state: RwLock<ServerState>,
    pub vrf_key: Arc<VRFPrivKey>,
    pub vrf_range_contract: OperatorRangeContract,
    pub sender: Option<WebsocketSender>,
    pub receiver: Option<WebsocketReceiver>,
    pub hub_state: Option<SharedState>,
}

pub type OperatorArc = Arc<Mutex<Operator>>;
//TODO use RwLock
//pub type OperatorArc = Arc<RwLock<Operator>>;

impl Operator {
    pub fn operator_factory() -> OperatorFactory {
        OperatorFactory::init()
    }
}

/// A cache state of a server node.
#[derive(Debug, Clone)]
pub struct ServerState {
    // pub clock_info: ClockInfo,
    pub signer_key: B256,
    pub message_ids: VecDeque<String>,
    pub cache_maximum: u64,
}

impl ServerState {
    /// Create a new server state.
    pub fn new(signer: B256, node_id: String, cache_maximum: u64) -> Self {
        Self {
            signer_key: signer,
            message_ids: VecDeque::new(),
            cache_maximum,
        }
    }
}
