use crate::api::vrf_key::VRFPrivKey;
use crate::error::{
    OperatorError::{
        OPDecodeSignerKeyError, OPNewVrfRangeContractError, OPVrfMaterialError, OpChannelError,
    },
    OperatorResult,
};
use crate::operator::{Operator, OperatorArc, ServerState};
use crate::storage;
use alloy_primitives::{hex::FromHex, B256};
use alloy_wrapper::contracts::vrf_range::new_vrf_range_backend;
use node_api::config::OperatorConfig;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, RwLock};
use verify_hub::server::server;
use vrf::ecvrf::VRFPrivateKey;

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
        let vrf_range_contract = new_vrf_range_backend(
            &config.chain.chain_rpc_url,
            &config.chain.vrf_range_contract,
        )
        .map_err(OPNewVrfRangeContractError)?;

        let vrf_key =
            VRFPrivateKey::try_from(config.node.vrf_key.as_str()).map_err(OPVrfMaterialError)?;

        let node_id = config.node.node_id.clone();
        let signer_key =
            B256::from_hex(config.node.signer_key.clone()).map_err(OPDecodeSignerKeyError)?;
        let server_state = ServerState::new(signer_key, node_id, config.node.cache_msg_maximum);
        let state = RwLock::new(server_state);

        let cfg = Arc::new(config);
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

    pub async fn initialize_node(&self) -> OperatorResult<OperatorArc> {
        let arc_operator = OperatorFactory::create_operator(self.config.clone()).await?;

        let (tx, rx) = oneshot::channel();

        let config = self.config.clone();
        tokio::task::spawn(async move {
            server::run(&config.net.rest_url, tx).await;
        });

        let server = rx.await.map_err(OpChannelError)?;
        arc_operator.lock().await.hub_state = Some(server);

        Ok(arc_operator)
    }
}
