mod private {
    pub mod milvus {
        tonic::include_proto!("milvus.proto.milvus");
    }

    pub mod schema {
        tonic::include_proto!("milvus.proto.schema");
    }

    pub mod common {
        tonic::include_proto!("milvus.proto.common");
    }
}

pub use private::common::{ConsistencyLevel, MsgBase, Status};
pub use private::milvus as api;
use private::milvus::{
    BoolResponse, CreatePartitionRequest, DescribeCollectionRequest, DescribeCollectionResponse,
    DropCollectionRequest, DropPartitionRequest, FlushRequest, FlushResponse, HasPartitionRequest,
    InsertRequest, LoadCollectionRequest, LoadPartitionsRequest, MutationResult,
};
pub use private::schema;
pub use tonic::transport::Endpoint;

use prost::Message;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tonic Transport Error: {0}")]
    TonicTransport(#[from] tonic::transport::Error),

    #[error("Tonic Status Error: {0}")]
    TonicStatus(#[from] tonic::Status),

    #[error("Error: Row number is misaligned!")]
    RowNumMisaligned,
}

pub struct Client {
    base: Option<MsgBase>,
    db_name: String,
    inner: api::milvus_service_client::MilvusServiceClient<tonic::transport::Channel>,
}

impl Client {
    pub async fn connect<E: Into<tonic::transport::Endpoint>>(
        db_name: String,
        url: E,
    ) -> Result<Self, Error> {
        let endpoint = url.into();
        let inner = api::milvus_service_client::MilvusServiceClient::connect(endpoint).await?;

        Ok(Self {
            base: None,
            db_name,
            inner,
        })
    }

    #[inline]
    pub async fn show_collections(
        &mut self,
        names: Option<Vec<String>>,
    ) -> Result<api::ShowCollectionsResponse, Error> {
        let list_collections_request = api::ShowCollectionsRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            time_stamp: 0,
            r#type: api::ShowType::InMemory.into(),
            collection_names: names.unwrap_or_else(Vec::new),
        };

        Ok(self
            .inner
            .show_collections(list_collections_request)
            .await?
            .into_inner())
    }

    pub async fn create_collection<N: Into<String>>(
        &mut self,
        name: N,
        schema: schema::CollectionSchema,
        cl: Option<ConsistencyLevel>,
        shards_num: Option<i32>,
    ) -> Result<Status, Error> {
        let req = api::CreateCollectionRequest {
            base: self.base.clone(), // ::core::option::Option<super::common::MsgBase>,
            db_name: self.db_name.clone(), // ::prost::alloc::string::String,
            collection_name: name.into(), // ::prost::alloc::string::String,
            schema: schema.encode_length_delimited_to_vec(), // ::prost::alloc::vec::Vec<u8>,
            shards_num: shards_num.unwrap_or(0), // i32,
            consistency_level: cl.unwrap_or(ConsistencyLevel::Session).into(),
        };

        Ok(self.inner.create_collection(req).await?.into_inner())
    }

    pub async fn load_collection<C: Into<String>>(
        &mut self,
        collection_name: C,
    ) -> Result<Status, Error> {
        let req = LoadCollectionRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            replica_number: 0,
        };

        Ok(self.inner.load_collection(req).await?.into_inner())
    }

    pub async fn describe_collection<C: Into<String>>(
        &mut self,
        collection_name: C,
    ) -> Result<DescribeCollectionResponse, Error> {
        let req = DescribeCollectionRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            collection_id: 0,
            time_stamp: 0,
        };

        Ok(self.inner.describe_collection(req).await?.into_inner())
    }

    pub async fn drop_collection<C: Into<String>>(
        &mut self,
        collection_name: C,
    ) -> Result<Status, Error> {
        let req = DropCollectionRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
        };

        Ok(self.inner.drop_collection(req).await?.into_inner())
    }
    pub async fn create_partition<C: Into<String>, P: Into<String>>(
        &mut self,
        collection_name: C,
        partition_name: P,
    ) -> Result<Status, Error> {
        let req = CreatePartitionRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            partition_name: partition_name.into(),
        };

        Ok(self.inner.create_partition(req).await?.into_inner())
    }

    pub async fn has_partition<C: Into<String>, P: Into<String>>(
        &mut self,
        collection_name: C,
        partition_name: P,
    ) -> Result<BoolResponse, Error> {
        let req = HasPartitionRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            partition_name: partition_name.into(),
        };

        Ok(self.inner.has_partition(req).await?.into_inner())
    }

    pub async fn load_partitions<I, C, P>(
        &mut self,
        collection_name: C,
        partition_names: P,
    ) -> Result<Status, Error>
    where
        C: Into<String>,
        I: ToString,
        P: IntoIterator<Item = I>,
    {
        let req = LoadPartitionsRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            partition_names: partition_names.into_iter().map(|x| x.to_string()).collect(),
            replica_number: 0,
        };

        Ok(self.inner.load_partitions(req).await?.into_inner())
    }
    pub async fn drop_partition<C: Into<String>, P: Into<String>>(
        &mut self,
        collection_name: C,
        partition_name: P,
    ) -> Result<Status, Error> {
        let req = DropPartitionRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            partition_name: partition_name.into(),
        };

        Ok(self.inner.drop_partition(req).await?.into_inner())
    }

    pub async fn insert<C: Into<String>, P: Into<String>>(
        &mut self,
        collection_name: C,
        partition_name: Option<P>,
        fields_data: Vec<schema::FieldData>,
    ) -> Result<MutationResult, Error> {
        let cols = fields_data.iter().map(|fd| {
            use schema::field_data::Field;
            use schema::scalar_field::Data as ScalarData;
            use schema::vector_field::Data as VectorData;

            fd.field.as_ref().and_then(|f| match f {
                Field::Scalars(s) => s.data.as_ref().map(|d| match d {
                    ScalarData::BoolData(d) => d.data.len(),
                    ScalarData::IntData(d) => d.data.len(),
                    ScalarData::LongData(d) => d.data.len(),
                    ScalarData::FloatData(d) => d.data.len(),
                    ScalarData::DoubleData(d) => d.data.len(),
                    ScalarData::StringData(d) => d.data.len(),
                    ScalarData::BytesData(d) => d.data.len(),
                }),
                Field::Vectors(v) => v.data.as_ref().and_then(|d| match d {
                    VectorData::FloatVector(fv) => Some(fv.data.len() / v.dim as usize),
                    VectorData::BinaryVector(_) => None,
                }),
            })
        });

        let mut num_rows = 0;
        for col in cols {
            if let Some(nr) = col {
                if num_rows == 0 {
                    num_rows = nr;
                } else if num_rows != nr {
                    return Err(Error::RowNumMisaligned);
                }
            }
        }

        let req = InsertRequest {
            base: self.base.clone(),
            db_name: self.db_name.clone(),
            collection_name: collection_name.into(),
            partition_name: partition_name
                .map(|pn| pn.into())
                .unwrap_or_else(String::new),
            fields_data,
            hash_keys: Vec::new(),
            num_rows: num_rows as u32,
        };

        Ok(self.inner.insert(req).await?.into_inner())
    }

    pub async fn query<C, E, F, P>(
        &mut self,
        collection_name: C,
        expr: E,
        output_fields: F,
        partition_names: P,
    ) -> Result<api::QueryResults, Error>
    where
        C: ToString,
        E: ToString,
        F: IntoIterator,
        F::Item: ToString,
        P: IntoIterator,
        P::Item: ToString,
    {
        let req = api::QueryRequest {
            base: None,
            db_name: self.db_name.clone(),
            collection_name: collection_name.to_string(),
            expr: expr.to_string(),
            output_fields: output_fields.into_iter().map(|x| x.to_string()).collect(),
            partition_names: partition_names.into_iter().map(|x| x.to_string()).collect(),
            guarantee_timestamp: 0,
            travel_timestamp: 0,
        };

        Ok(self.inner.query(req).await?.into_inner())
    }

    pub async fn flush<C>(&mut self, collections: C) -> Result<FlushResponse, Error>
    where
        C: IntoIterator,
        C::Item: ToString,
    {
        let req = FlushRequest {
            db_name: self.db_name.clone(),
            base: None,
            collection_names: collections.into_iter().map(|x| x.to_string()).collect(),
        };

        Ok(self.inner.flush(req).await?.into_inner())
    }
}
