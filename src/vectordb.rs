// src/vectordb.rs

use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::PointId;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, GetCollectionInfoResponse, PointStruct, QueryPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant, QdrantError};
use serde_json::json;
use std::convert::TryInto;

/// A simple wrapper around a connected Qdrant client instance.
pub struct VectorDB {
    /// Expose the underlying Qdrant client for additional operations.
    pub client: Qdrant,
}

/// A record to be stored in the vector database.
#[derive(Debug)]
pub struct Record {
    pub id: u64,
    pub local_path: String,
    pub virtual_path: String,
    pub embedding: Vec<f32>,
}

/// A search result returned from a vector query.
#[derive(Debug)]
pub struct SearchResult {
    pub id: u64,
    pub score: f32,
    pub local_path: Option<String>,
    pub virtual_path: Option<String>,
}

impl VectorDB {
    fn point_id_to_u64(point_id: PointId) -> Result<u64, QdrantError> {
        match point_id.point_id_options {
            Some(PointIdOptions::Num(n)) => Ok(n),
            Some(PointIdOptions::Uuid(_)) => Err(QdrantError::ConversionError(
                "Expected numeric id but got UUID".to_string(),
            )),
            None => Err(QdrantError::ConversionError(
                "Missing point id options".to_string(),
            )),
        }
    }

    /// Connect to a Qdrant instance and return a new `VectorDB`.
    ///
    /// # Arguments
    ///
    /// * `url` - The address of your Qdrant instance (e.g., "http://localhost:6334").
    ///
    /// # Returns
    ///
    /// A Result containing a connected `VectorDB` or a `QdrantError`.
    pub fn new(url: &str) -> Result<Self, QdrantError> {
        let client = Qdrant::from_url(url).build()?;
        Ok(Self { client })
    }

    /// Ensures that a collection exists in Qdrant.
    ///
    /// This method first checks if the collection exists. If it exists and its status is
    /// either 0 (green) or 1 (yellow), it returns successfully. If the collection does not
    /// exist (or returns an error indicating "not found"), then it creates a new collection.
    /// After creation, it verifies that the collection’s status is acceptable (green or yellow).
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the collection.
    /// * `dims` - The dimensionality of the vectors.
    /// * `distance` - The distance metric to use (e.g., `Distance::Cosine`).
    ///
    /// # Returns
    ///
    /// A Result with unit type on success or a `QdrantError` on failure.
    pub async fn ensure_collection_exists(
        &self,
        name: &str,
        dims: usize,
        distance: Distance,
    ) -> Result<(), QdrantError> {
        // Check if the collection exists.
        match self.collection_info(name).await {
            Ok(response) => {
                if let Some(info) = response.result {
                    // Accept status 0 (green) or 1 (yellow) as healthy.
                    if info.status == 0 || info.status == 1 {
                        return Ok(());
                    } else {
                        return Err(QdrantError::ConversionError(format!(
                            "Collection {} exists but status is {}",
                            name, info.status
                        )));
                    }
                }
                // If result is None, fall through to creation.
            }
            Err(err) => {
                // If the error does not indicate "not found", return the error.
                if !err.to_string().contains("not found") {
                    return Err(err);
                }
            }
        }

        // Collection does not exist: create a new collection.
        self.client
            .create_collection(
                CreateCollectionBuilder::new(name)
                    .vectors_config(VectorParamsBuilder::new(dims.try_into().unwrap(), distance)),
            )
            .await?;

        // Verify that the collection now exists.
        let new_response = self.collection_info(name).await?;
        if let Some(new_info) = new_response.result {
            if new_info.status == 0 || new_info.status == 1 {
                Ok(())
            } else {
                Err(QdrantError::ConversionError(format!(
                    "Collection {} created but status is {}",
                    name, new_info.status
                )))
            }
        } else {
            Err(QdrantError::ConversionError(format!(
                "Collection {} info missing",
                name
            )))
        }
    }

    /// Retrieves collection information for the specified collection.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the collection.
    ///
    /// # Returns
    ///
    /// A Result containing a `GetCollectionInfoResponse` or a `QdrantError`.
    pub async fn collection_info(
        &self,
        name: &str,
    ) -> Result<GetCollectionInfoResponse, QdrantError> {
        self.client.collection_info(name).await
    }

    /// Adds a list of records to the specified collection.
    ///
    /// For each record, it constructs a payload containing the `local_path` and `virtual_path`
    /// and uploads the record with its embedding vector.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The target collection name.
    /// * `records` - A vector of records to add.
    ///
    /// # Returns
    ///
    /// A Result with unit type on success or a `QdrantError` on failure.
    pub async fn add_records(
        &self,
        collection_name: &str,
        records: Vec<Record>,
    ) -> Result<(), QdrantError> {
        let mut points = Vec::with_capacity(records.len());
        for record in records {
            let payload = Payload::try_from(json!({
                "local_path": record.local_path,
                "virtual_path": record.virtual_path,
            }))
            .map_err(|e| QdrantError::ConversionError(e.to_string()))?;

            points.push(PointStruct::new(record.id, record.embedding, payload));
        }

        self.client
            .upsert_points(UpsertPointsBuilder::new(collection_name, points).wait(true))
            .await?;
        Ok(())
    }

    /// Performs a vector search on the specified collection.
    ///
    /// This method queries the collection using the provided vector, requesting the payload
    /// so that it can extract the `local_path` and `virtual_path` fields along with the score.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - The name of the collection to query.
    /// * `query_vector` - The query vector.
    ///
    /// # Returns
    ///
    /// A Result containing a vector of `SearchResult` or a `QdrantError` on failure.
    pub async fn query_by_vector(
        &self,
        collection_name: &str,
        query_vector: Vec<f32>,
    ) -> Result<Vec<SearchResult>, QdrantError> {
        let query_response = self
            .client
            .query(
                QueryPointsBuilder::new(collection_name)
                    .query(query_vector)
                    .with_payload(true),
            )
            .await?;

        // query_response.result is already a Vec<ScoredPoint>
        let points = query_response.result;

        let mut results = Vec::with_capacity(points.len());
        for point in points {
            // Extract payload fields using try_get.
            let local_path = point
                .try_get("local_path")
                .and_then(|v| v.as_str())
                .map(String::from);
            let virtual_path = point
                .try_get("virtual_path")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Convert the point id from PointId to u64.
            let id = point
                .id
                .ok_or_else(|| {
                    QdrantError::ConversionError("Missing point id in search result".to_string())
                })
                .and_then(Self::point_id_to_u64)?;

            results.push(SearchResult {
                id,
                score: point.score,
                local_path,
                virtual_path,
            });
        }
        Ok(results)
    }
}
