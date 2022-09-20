use crate::data::SearchResults;
use crate::error::{Error as SuperError, Result};
use crate::proto::common::{
    DslType, KeyValuePair, MsgType, PlaceholderGroup, PlaceholderType, PlaceholderValue,
};
use crate::proto::milvus::milvus_service_client::MilvusServiceClient;
use crate::proto::milvus::{
    CreateCollectionRequest, CreatePartitionRequest, DropCollectionRequest, FlushRequest,
    HasCollectionRequest, HasPartitionRequest, InsertRequest, LoadCollectionRequest, QueryRequest,
    ReleaseCollectionRequest, SearchRequest, ShowCollectionsRequest, ShowPartitionsRequest,
    ShowType,
};
use crate::proto::schema::{DataType, SearchResultData};
use crate::utils::{new_msg, status_to_result};
use crate::{config, schema};

use core::fmt;
use prost::bytes::{BufMut, BytesMut};
use prost::Message;
use std::borrow::Cow;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::time::Duration;
use tokio::sync::Mutex;
use tonic::transport::Channel;

pub use crate::proto::common::ConsistencyLevel;

#[derive(Debug)]
pub struct Partition {
    pub name: Cow<'static, str>,
    pub percentage: i64,
}

#[derive(Debug)]
pub struct Collection<C> {
    client: MilvusServiceClient<Channel>,
    name: Cow<'static, str>,
    partitions: Mutex<HashSet<String>>,
    _m: PhantomData<C>,
}

impl<C> Collection<C> {
    pub fn new<N: Into<Cow<'static, str>>>(client: MilvusServiceClient<Channel>, name: N) -> Self {
        Self {
            client,
            name: name.into(),
            partitions: Mutex::new(Default::default()),
            _m: Default::default(),
        }
    }

    async fn load(&self, replica_number: i32) -> Result<()> {
        status_to_result(Some(
            self.client
                .clone()
                .load_collection(LoadCollectionRequest {
                    base: Some(new_msg(MsgType::LoadCollection)),
                    db_name: "".to_string(),
                    collection_name: self.name.to_string(),
                    replica_number,
                })
                .await?
                .into_inner(),
        ))
    }

    pub async fn load_unblocked(&self, replica_number: i32) -> Result<()> {
        dbg!("start load_unblocked");
        // TODO wrap the error
        // let rt = Builder::new_current_thread().enable_all().build().unwrap();
        // rt.block_on(self.load(replica_number))
        self.load(replica_number).await
    }

    pub async fn get_load_percent(&self) -> Result<i64> {
        let response = self
            .client
            .clone()
            .show_collections(ShowCollectionsRequest {
                base: Some(new_msg(MsgType::ShowCollections)),
                db_name: "".to_string(),
                time_stamp: 0,
                r#type: ShowType::InMemory as i32,
                collection_names: vec![self.name.to_string()],
            })
            .await?
            .into_inner();

        status_to_result(response.status)?;

        let names = response.collection_names;
        let percent = response.in_memory_percentages;
        for i in 0..names.len() {
            if self.name == names[i] {
                return Ok(percent[i]);
            }
        }

        Err(SuperError::Unknown)
    }

    pub async fn load_blocked(&self, replica_number: i32) -> Result<()> {
        self.load(replica_number).await?;

        loop {
            if self.get_load_percent().await? >= 100 {
                return Ok(());
            }

            tokio::time::sleep(Duration::from_millis(config::WAIT_LOAD_DURATION_MS)).await;
        }
    }

    pub async fn is_load(&self) -> Result<bool> {
        Ok(self.get_load_percent().await? >= 100)
    }

    pub async fn release(&self) -> Result<()> {
        status_to_result(Some(
            self.client
                .clone()
                .release_collection(ReleaseCollectionRequest {
                    base: Some(new_msg(MsgType::ReleaseCollection)),
                    db_name: "".to_string(),
                    collection_name: self.name.to_string(),
                })
                .await?
                .into_inner(),
        ))
    }

    pub async fn drop(&self) -> Result<()> {
        status_to_result(Some(
            self.client
                .clone()
                .drop_collection(DropCollectionRequest {
                    base: Some(new_msg(MsgType::DropCollection)),
                    db_name: "".to_string(),
                    collection_name: self.name.to_string(),
                })
                .await?
                .into_inner(),
        ))
    }

    pub async fn exists(&self) -> Result<bool> {
        let res = self
            .client
            .clone()
            .has_collection(HasCollectionRequest {
                base: Some(new_msg(MsgType::HasCollection)),
                db_name: "".to_string(),
                collection_name: self.name.to_string(),
                time_stamp: 0,
            })
            .await?
            .into_inner();

        status_to_result(res.status)?;

        Ok(res.value)
    }

    pub async fn flush(&self) -> Result<()> {
        let res = self
            .client
            .clone()
            .flush(FlushRequest {
                base: Some(new_msg(MsgType::Flush)),
                db_name: "".to_string(),
                collection_names: vec![self.name.to_string()],
            })
            .await?
            .into_inner();

        status_to_result(res.status)?;

        Ok(())
    }

    pub async fn load_partition_list(&self) -> Result<()> {
        let res = self
            .client
            .clone()
            .show_partitions(ShowPartitionsRequest {
                base: Some(new_msg(MsgType::ShowPartitions)),
                db_name: "".to_string(),
                collection_name: self.name.to_string(),
                collection_id: 0,
                partition_names: Vec::new(),
                r#type: 0,
            })
            .await?
            .into_inner();

        let mut partitions = HashSet::new();
        for name in res.partition_names {
            partitions.insert(name);
        }

        std::mem::swap(&mut *self.partitions.lock().await, &mut partitions);

        status_to_result(res.status)?;

        Ok(())
    }

    pub async fn create_partition(&self, name: String) -> Result<()> {
        let res = self
            .client
            .clone()
            .create_partition(CreatePartitionRequest {
                base: Some(new_msg(MsgType::ShowPartitions)),
                db_name: "".to_string(),
                collection_name: self.name.to_string(),
                partition_name: name,
            })
            .await?
            .into_inner();

        status_to_result(Some(res))?;

        Ok(())
    }

    pub async fn has_partition<P: AsRef<str>>(&self, p: P) -> Result<bool> {
        if self.partitions.lock().await.contains(p.as_ref()) {
            return Ok(true);
        } else {
            let res = self
                .client
                .clone()
                .has_partition(HasPartitionRequest {
                    base: Some(new_msg(MsgType::HasPartition)),
                    db_name: "".to_string(),
                    collection_name: self.name.to_string(),
                    partition_name: p.as_ref().to_string(),
                })
                .await?
                .into_inner();

            status_to_result(res.status)?;

            Ok(res.value)
        }
    }
}

impl<E: schema::Entity> Collection<E> {
    pub async fn create(
        &self,
        shards_num: Option<i32>,
        consistency_level: Option<ConsistencyLevel>,
    ) -> Result<()> {
        let schema: crate::proto::schema::CollectionSchema = E::schema().into();

        let status = self
            .client
            .clone()
            .create_collection(CreateCollectionRequest {
                base: Some(new_msg(MsgType::CreateCollection)),
                db_name: "".to_string(),
                collection_name: schema.name.to_string(),
                schema: schema.encode_to_vec(),
                shards_num: shards_num.unwrap_or(1),
                consistency_level: consistency_level.unwrap_or(ConsistencyLevel::Session) as i32,
            })
            .await?
            .into_inner();

        status_to_result(Some(status))
    }

    pub async fn query<'a, Exp, F, P>(&self, expr: Exp, partition_names: P) -> Result<F>
    where
        Exp: ToString,
        F: schema::Collection<'a, Entity = E> + schema::FromDataFields,
        P: IntoIterator,
        P::Item: ToString,
    {
        let res = self
            .client
            .clone()
            .query(QueryRequest {
                base: Some(new_msg(MsgType::Retrieve)),
                db_name: "".to_string(),
                collection_name: self.name.to_string(),
                expr: expr.to_string(),
                output_fields: F::columns()
                    .into_iter()
                    .map(|x| x.name.to_string())
                    .collect(),
                partition_names: partition_names.into_iter().map(|x| x.to_string()).collect(),
                guarantee_timestamp: 0,
                travel_timestamp: 0,
                query_params: vec![],
                consistency_level: ConsistencyLevel::Session as _,
                use_default_consistency: true,
            })
            .await?
            .into_inner();

        status_to_result(res.status)?;

        Ok(F::from_data_fields(res.fields_data).unwrap())
    }

    async fn search_inner<'a, A: SearchArray>(
        &self,
        dsl: Option<String>,
        query: &[A],
        output_fields: Vec<String>,
        partition_names: Vec<String>,
        search_params: Vec<KeyValuePair>,
    ) -> Result<Option<SearchResultData>> {
        let schema = E::schema();
        let vector_field = schema.vector_field().unwrap();

        let pg = PlaceholderGroup {
            placeholders: vec![PlaceholderValue {
                tag: "$0".to_string(),
                r#type: match vector_field.dtype {
                    DataType::FloatVector => PlaceholderType::FloatVector,
                    DataType::BinaryVector => PlaceholderType::BinaryVector,
                    _ => PlaceholderType::None,
                } as _,
                values: query.into_iter().map(|a| a.serialize()).collect(),
            }],
        };

        let res = self
            .client
            .clone()
            .search(SearchRequest {
                base: Some(new_msg(MsgType::Search)),
                db_name: "".to_string(),
                collection_name: self.name.to_string(),
                dsl: dsl.unwrap_or_else(|| "".to_string()),
                dsl_type: DslType::BoolExprV1 as _,
                output_fields,
                partition_names,
                placeholder_group: pg.encode_to_vec(),
                search_params,
                nq: 0,
                guarantee_timestamp: 0,
                travel_timestamp: 0,
                consistency_level: ConsistencyLevel::Session as _,
                use_default_consistency: true,
            })
            .await?
            .into_inner();

        status_to_result(res.status)?;

        Ok(res.results)
    }

    pub async fn search<'a, Exp, F, P, A>(
        &self,
        expr: Option<Exp>,
        query: &[A],
        partition_names: P,
        params: SearchParams,
    ) -> Result<SearchResults<'a, F>>
    where
        A: SearchArray,
        Exp: ToString,
        F: schema::Collection<'a, Entity = E> + schema::FromDataFields,
        P: IntoIterator,
        P::Item: ToString,
    {
        let schema = E::schema();
        let vector_field = schema.vector_field().unwrap();
        let est_row_size: usize = F::columns().iter().map(|c| c.estimate_size()).sum();
        let max_batch_size = 1 + ((MAX_SEARCH_TRANSACTION_SIZE - 1) / est_row_size);

        let output_fileds: Vec<_> = F::columns()
            .into_iter()
            .map(|x| x.name.to_string())
            .collect();

        let partition_names = partition_names.into_iter().map(|x| x.to_string()).collect();

        let search_params = [
            ("anns_field", vector_field.name.to_string()),
            ("topk", format!("{}", params.top_k)),
            ("params", format!("{{}}")),
            ("metric_type", format!("{}", params.metric_type)),
            ("round_decimal", format!("-1")),
        ]
        .into_iter()
        .map(|(k, value)| KeyValuePair {
            key: k.to_string(),
            value,
        })
        .collect();

        let mut chunks_iter = query.chunks(max_batch_size);
        let z = chunks_iter.size_hint().1;
        let mut collection = SearchResults::with_capacity(0);
        let expr = expr.map(|s| s.to_string());

        if let Some(1) = z {
            if let Some(dat) = self
                .search_inner(
                    expr,
                    chunks_iter.next().unwrap(),
                    output_fileds,
                    partition_names,
                    search_params,
                )
                .await?
            {
                collection.append_search_result_data(dat);
            }
        } else {
            let results = futures::future::join_all(chunks_iter.map(|chunk| {
                self.search_inner(
                    expr.clone(),
                    chunk,
                    output_fileds.clone(),
                    partition_names.clone(),
                    search_params.clone(),
                )
            }))
            .await;

            for res in results {
                match res {
                    Ok(Some(dat)) => collection.append_search_result_data(dat),
                    Ok(None) => continue,
                    Err(err) => return Err(err.into()),
                }
            }
        }

        Ok(collection)
    }

    pub async fn insert<'a, P: Into<String>, C: schema::Collection<'a, Entity = E>>(
        &self,
        fields_data: C,
        partition_name: Option<P>,
    ) -> Result<crate::proto::milvus::MutationResult> {
        let partition_name = if let Some(p) = partition_name {
            let p = p.into();

            if !self.has_partition(&p).await? {
                self.create_partition(p.clone()).await?;
            }

            p
        } else {
            String::new()
        };

        Ok(self
            .client
            .clone()
            .insert(InsertRequest {
                base: Some(new_msg(MsgType::Insert)),
                db_name: "".to_string(),
                collection_name: self.name.to_string(),
                partition_name,
                num_rows: fields_data.len() as _,
                fields_data: fields_data.into_data_fields(),
                hash_keys: Vec::new(),
            })
            .await?
            .into_inner())
    }
}

const MAX_SEARCH_TRANSACTION_SIZE: usize = 5 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SearchQueryOption {
    pub consistency_level: ConsistencyLevel,
    pub guarantee_timestamp: u64,
    pub travel_timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    Flat,
    BinFlat,
    IvfFlat,
    BinIvfFlat,
    IvfPQ,
    IvfSQ8,
    IvfSQ8H,
    NSG,
    HNSW,
    RHNSWFlat,
    RHNSWPQ,
    RHNSWSQ,
    IvfHNSW,
    ANNOY,
    NGTPANNG,
    NGTONNG,
}

impl fmt::Display for IndexType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Flat => "FLAT", //faiss
                Self::BinFlat => "BIN_FLAT",
                Self::IvfFlat => "IVF_FLAT", //faiss
                Self::BinIvfFlat => "BIN_IVF_FLAT",
                Self::IvfPQ => "IVF_PQ", //faiss
                Self::IvfSQ8 => "IVF_SQ8",
                Self::IvfSQ8H => "IVF_SQ8_HYBRID",
                Self::NSG => "NSG",
                Self::HNSW => "HNSW",
                Self::RHNSWFlat => "RHNSW_FLAT",
                Self::RHNSWPQ => "RHNSW_PQ",
                Self::RHNSWSQ => "RHNSW_SQ",
                Self::IvfHNSW => "IVF_HNSW",
                Self::ANNOY => "ANNOY",
                Self::NGTPANNG => "NGT_PANNG",
                Self::NGTONNG => "NGT_ONNG",
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    L2,
    Ip,
    Hamming,
    Jaccard,
    Tanimoto,
    SubStructure,
    SuperStructure,
}

impl fmt::Display for MetricType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::L2 => "L2",
                Self::Ip => "IP",
                Self::Hamming => "HAMMING",
                Self::Jaccard => "JACCARD",
                Self::Tanimoto => "TANIMOTO",
                Self::SubStructure => "SUBSTRUCTURE",
                Self::SuperStructure => "SUPERSTRUCTURE",
            }
        )
    }
}

pub struct SearchParams {
    pub top_k: i32,
    pub metric_type: MetricType,
}

pub trait SearchArray {
    fn serialize(&self) -> Vec<u8>;
}

impl<'a> SearchArray for &'a [f32] {
    fn serialize(&self) -> Vec<u8> {
        let slice = self.as_ref();
        let mut buf = BytesMut::with_capacity(slice.len() * 4);

        for &f in slice {
            buf.put_f32_le(f);
        }

        buf.into()
    }
}

impl SearchArray for Vec<f32> {
    fn serialize(&self) -> Vec<u8> {
        self.as_slice().serialize()
    }
}

impl<const C: usize> SearchArray for [f32; C] {
    fn serialize(&self) -> Vec<u8> {
        self.as_slice().serialize()
    }
}

impl<'a> SearchArray for &'a [u8] {
    fn serialize(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl SearchArray for Vec<u8> {
    fn serialize(&self) -> Vec<u8> {
        self.clone()
    }
}

impl<const C: usize> SearchArray for [u8; C] {
    fn serialize(&self) -> Vec<u8> {
        self.to_vec()
    }
}
