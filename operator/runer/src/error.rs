use std::path::PathBuf;
use thiserror::Error;
use vrf::traits::CryptoMaterialError;

pub enum OperatorErrorCodes {
    OP_CUSTOM_ERROR = 3001,
    OP_FAIL_REGISTER = 3002,
    OP_CONNECT_TEE_ERROR = 3003,
    OP_SEND_PROMPT_ERROR = 3004,
    OP_DECODE_SIGNER_KEY_ERROR = 3005,
    OP_NEW_VRF_RANGE_CONTRACT_ERROR = 3006,
    OP_GET_RANGE_CONTRACT_ERROR = 3007,
    OP_CRYPTO_MATERIAL_ERROR = 3008,
    OP_CHANNEL_ERROR = 3009,
}

pub type OperatorResult<T> = Result<T, OperatorError>;

#[derive(Error, Debug)]
pub enum OperatorError {
    #[error(
        "Error: some error happened, detail: {0} (Error Code: {})",
        OperatorErrorCodes::OP_CUSTOM_ERROR as u32
    )]
    CustomError(String),

    #[error(
        "Error: register to dispatcher failed, detail: {0} (Error Code: {})",
        OperatorErrorCodes::OP_FAIL_REGISTER as u32
    )]
    OPSetupRegister(#[from] reqwest::Error),

    #[error(
        "Error: connect to tee service failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_CONNECT_TEE_ERROR as u32
    )]
    OPConnectTEEError(String),

    #[error(
        "Error: send promtp to tee service failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_SEND_PROMPT_ERROR as u32
    )]
    OPSendPromptError(String),

    #[error(
        "Error: decode signer private key error failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_DECODE_SIGNER_KEY_ERROR as u32
    )]
    OPDecodeSignerKeyError(#[from] alloy_primitives::hex::FromHexError),

    #[error(
        "Error: new vrf range contract failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_NEW_VRF_RANGE_CONTRACT_ERROR as u32
    )]
    OPNewVrfRangeContractError(#[from] eyre::ErrReport),

    #[error(
        "Error: get vrf range contract failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_GET_RANGE_CONTRACT_ERROR as u32
    )]
    OPGetVrfRangeContractError(String),

    #[error(
        "Error: crypto material failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_CRYPTO_MATERIAL_ERROR as u32
    )]
    OPVrfMaterialError(CryptoMaterialError),

    #[error(
        "Error: Channel recv failed, detail: {0}  (Error Code: {})",
        OperatorErrorCodes::OP_CHANNEL_ERROR as u32
    )]
    OpChannelError(#[from] tokio::sync::oneshot::error::RecvError),
}
