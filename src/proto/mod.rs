use prost::{bytes::BytesMut, Message};

use self::{common::PlaceholderGroup, schema::CollectionSchema};

#[path = "milvus.proto.common.rs"]
pub mod common;

#[path = "milvus.proto.milvus.rs"]
pub mod milvus;

#[path = "milvus.proto.schema.rs"]
pub mod schema;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("EncodeError: {0}")]
    EncodeError(#[from] prost::EncodeError),
}
