use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum ImuError {
    #[error("communication error")]
    CommunicationError,
    #[error("chip not found")]
    ChipNotFound,
    #[error("configuration error")]
    ConfigError,
    #[error("data not ready")]
    DataNotReady,
    #[error("missing resource")]
    MissingResource,
    #[error("unsupported configuration")]
    UnsupportedConfig,
    #[error("invalid target")]
    InvalidTarget,
}
