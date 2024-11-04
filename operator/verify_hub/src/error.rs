use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerifyHubError {
    #[error("Error: Failed to send OPML request: {0}")]
    SendOpmlRequestError(String),

    #[error("Error: Failed to send ZKML request: {0}")]
    SendZkmlRequestError(String),

    #[error("Error: Channel closed unexpectedly")]
    ChannelClosedError,

    #[error("Error: Requst Timeout")]
    RequestTimeoutError,
}
