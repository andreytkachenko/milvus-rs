use crate::collection::Collection;
use crate::error::{Error, Result};
pub use crate::proto::common::ConsistencyLevel;
use crate::proto::common::MsgType;
use crate::proto::milvus::milvus_service_client::MilvusServiceClient;
use crate::proto::milvus::{
    CreateCollectionRequest, DropCollectionRequest, FlushRequest, HasCollectionRequest,
};
use crate::schema::{self, CollectionSchema};
use crate::utils::{new_msg, status_to_result};
use prost::bytes::BytesMut;
use prost::Message;
use std::collections::HashMap;
use std::convert::TryInto;
use tonic::codegen::StdError;
use tonic::transport::Channel;

pub struct Client {
    client: MilvusServiceClient<Channel>,
}

impl Client {
    pub async fn new<D>(dst: D) -> Result<Self>
    where
        D: TryInto<tonic::transport::Endpoint>,
        D::Error: Into<StdError>,
    {
        match MilvusServiceClient::connect(dst).await {
            Ok(i) => Ok(Self { client: i }),
            Err(e) => Err(Error::Communication(e)),
        }
    }

    pub async fn create_collection<C>(
        &self,
        schema: CollectionSchema<'_>,
        shards_num: i32,
        consistency_level: ConsistencyLevel,
    ) -> Result<Collection<C>> {
        let schema: crate::proto::schema::CollectionSchema = schema.into();
        let mut buf = BytesMut::new();

        //TODO unwrap instead of panic
        schema.encode(&mut buf).unwrap();

        let status = self
            .client
            .clone()
            .create_collection(CreateCollectionRequest {
                base: Some(new_msg(MsgType::CreateCollection)),
                db_name: "".to_string(),
                collection_name: schema.name.to_string(),
                schema: buf.to_vec(),
                shards_num,
                consistency_level: consistency_level as i32,
            })
            .await?
            .into_inner();

        status_to_result(Some(status))?;

        Ok(Collection::new(
            self.client.clone(),
            schema.name.to_string(),
        ))
    }

    pub async fn drop_collection<S>(&self, name: S) -> Result<()>
    where
        S: Into<String>,
    {
        status_to_result(Some(
            self.client
                .clone()
                .drop_collection(DropCollectionRequest {
                    base: Some(new_msg(MsgType::DropCollection)),
                    db_name: "".to_string(),
                    collection_name: name.into(),
                })
                .await?
                .into_inner(),
        ))
    }

    pub async fn has_collection<S>(&self, name: S) -> Result<bool>
    where
        S: Into<String>,
    {
        let name = name.into();
        let res = self
            .client
            .clone()
            .has_collection(HasCollectionRequest {
                base: Some(new_msg(MsgType::HasCollection)),
                db_name: "".to_string(),
                collection_name: name.clone(),
                time_stamp: 0,
            })
            .await?
            .into_inner();

        status_to_result(res.status)?;

        Ok(res.value)
    }

    pub async fn get_collection<E: schema::Entity>(&self) -> Result<Collection<E>> {
        E::schema().validate()?;
        Ok(Collection::new(self.client.clone(), E::NAME))
    }

    pub async fn flush_collections<C>(&self, collections: C) -> Result<HashMap<String, Vec<i64>>>
    where
        C: IntoIterator,
        C::Item: ToString,
    {
        let res = self
            .client
            .clone()
            .flush(FlushRequest {
                base: Some(new_msg(MsgType::Flush)),
                db_name: "".to_string(),
                collection_names: collections.into_iter().map(|x| x.to_string()).collect(),
            })
            .await?
            .into_inner();

        status_to_result(res.status)?;

        Ok(res
            .coll_seg_i_ds
            .into_iter()
            .map(|(k, v)| (k, v.data))
            .collect())
    }
}
