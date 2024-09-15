use crate::api::vrf_key::VRFPrivKey;
use crate::operator::{Operator, OperatorArc, ServerState};
use crate::storage;
use alloy_primitives::hex::FromHex;
use alloy_primitives::B256;
use alloy_wrapper::contracts::vrf_range::new_vrf_range_backend;
use node_api::config::OperatorConfig;
use node_api::error::{
    OperatorError::{OPDecodeSignerKeyError, OPNewVrfRangeContractError},
    OperatorResult,
};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::{Mutex, RwLock};
use tracing::info;
use verify_hub::server::server;
use vrf::ecvrf::VRFPrivateKey;
use websocket::*;

#[derive(Default)]
pub struct OperatorFactory {
    pub config: OperatorConfig,
}

impl OperatorFactory {
    pub fn init() -> Self {
        Self::default()
    }

    pub fn set_config(mut self, config: OperatorConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn create_operator(config: OperatorConfig) -> OperatorResult<OperatorArc> {
        let cfg = Arc::new(config.clone());
        let node_id = config.node.node_id.clone();
        let signer_key =
            B256::from_hex(config.node.signer_key.clone()).map_err(OPDecodeSignerKeyError)?;
        let vrf_range_contract = new_vrf_range_backend(
            &config.chain.chain_rpc_url,
            &config.chain.vrf_range_contract,
        )
        .map_err(OPNewVrfRangeContractError)?;

        let vrf_key = VRFPrivateKey::try_from(config.node.vrf_key.as_str()).unwrap();
        info!("vrf key ----- {:?}", vrf_key);

        let server_state = ServerState::new(signer_key, node_id, cfg.node.cache_msg_maximum);
        let state = RwLock::new(server_state);
        let storage = storage::Storage::new(cfg.clone()).await;
        let operator = Operator {
            config: cfg,
            storage,
            state,
            vrf_key: Arc::new(VRFPrivKey(vrf_key)),
            vrf_range_contract,
            sender: None,
            receiver: None,
            hub_state: None,
        };

        Ok(Arc::new(Mutex::new(operator)))
    }

    async fn create_ws_node(arc_operator: OperatorArc) {
        let _arc_operator_clone = Arc::clone(&arc_operator);

        let socket = SocketAddr::from_str("13.215.49.139:3000").unwrap();
        //let socket = SocketAddr::from_str("127.0.0.1:8080").unwrap();
        let (send, mut recv) = connect(Arc::new(WebsocketConfig::default()), socket)
            .await
            .unwrap();
        _arc_operator_clone.lock().await.sender = Some(send);
        _arc_operator_clone.lock().await.receiver = Some(recv);
    }

    pub async fn initialize_node(self) -> OperatorResult<OperatorArc> {
        let arc_operator = OperatorFactory::create_operator(self.config.clone()).await?;
        OperatorFactory::create_ws_node(arc_operator.clone()).await;

        let (tx, rx) = oneshot::channel();
        let _task = tokio::task::spawn(async move {
            server::run("0.0.0.0:21001", tx).await;
        });

        let server = rx.await.unwrap();
        arc_operator.lock().await.hub_state = Some(server);

        Ok(arc_operator)
    }
}
