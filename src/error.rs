use crate::proto;
use crate::proto::common::{ErrorCode, Status};
use crate::schema::Error as SchemaError;
use std::error::Error as OtherError;
use std::result;
use thiserror::Error;
use tonic::transport::Error as CommError;
use tonic::Status as GrpcError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0:?}")]
    Other(#[from] Box<dyn OtherError + Send + Sync>),

    #[error("{0:?}")]
    ProtoError(#[from] proto::Error),

    #[error("{0:?}")]
    Communication(#[from] CommError),

    #[error("{0:?}")]
    Grpc(#[from] GrpcError),

    #[error("{0:?}")]
    Schema(#[from] SchemaError),

    #[error("{0:?} {1:?}")]
    Server(ErrorCode, String),

    #[error("Prost encode error: {0:?}")]
    ProstEncode(#[from] prost::EncodeError),

    #[error("Prost decode error: {0:?}")]
    ProstDecode(#[from] prost::DecodeError),

    #[error("Unknown")]
    Unknown,
}

impl From<Status> for Error {
    fn from(s: Status) -> Self {
        Error::Server(ErrorCode::from_i32(s.error_code).unwrap(), s.reason)
    }
}

pub type Result<T> = result::Result<T, Error>;
