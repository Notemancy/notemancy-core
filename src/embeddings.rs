use anyhow::{Context, Result};
use arrow_array::Array;
use arrow_array::{
    types::Float32Type, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch,
    RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A struct to represent document embeddings with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEmbedding {
    /// Unique identifier for the embedding
    pub id: String,
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Metadata associated with this embedding
    pub metadata: HashMap<String, String>,
}

/// Configuration for the embedding database
#[derive(Debug, Clone)]
pub struct EmbeddingStoreConfig {
    /// Path to the database
    pub db_path: String,
    /// Dimension of the embeddings
    pub embedding_dim: usize,
}

/// A trait defining the operations for an embedding store
#[async_trait]
pub trait EmbeddingStore {
    /// Initialize a new connection to the embedding store
    async fn connect(config: EmbeddingStoreConfig) -> Result<Self>
    where
        Self: Sized;

    /// Create a new embedding table
    async fn create_table(&self, table_name: &str) -> Result<()>;

    /// Drop an existing embedding table
    async fn drop_table(&self, table_name: &str) -> Result<()>;

    /// Check if a table exists
    async fn table_exists(&self, table_name: &str) -> Result<bool>;

    /// Add embeddings to a table
    async fn add_embeddings(
        &self,
        table_name: &str,
        embeddings: Vec<DocumentEmbedding>,
    ) -> Result<()>;

    /// Delete embeddings by their IDs
    async fn delete_embeddings(&self, table_name: &str, ids: Vec<String>) -> Result<()>;

    /// Find similar embeddings using vector similarity search
    async fn similarity_search(
        &self,
        table_name: &str,
        query_vector: Vec<f32>,
        limit: usize,
        metadata_filter: Option<&str>,
    ) -> Result<Vec<(DocumentEmbedding, f32)>>;
}

/// Implementation of the EmbeddingStore trait using LanceDB
pub struct LanceDBStore {
    connection: Connection,
    embedding_dim: usize,
}

#[async_trait]
impl EmbeddingStore for LanceDBStore {
    async fn connect(config: EmbeddingStoreConfig) -> Result<Self> {
        let connection = connect(&config.db_path)
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        Ok(Self {
            connection,
            embedding_dim: config.embedding_dim,
        })
    }

    async fn create_table(&self, table_name: &str) -> Result<()> {
        // First check if the table already exists
        if self.table_exists(table_name).await? {
            return Ok(());
        }

        // Define the schema for the embedding table
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.embedding_dim as i32,
                ),
                true,
            ),
            // Add a field for metadata as JSON
            Field::new("metadata_json", DataType::Utf8, true),
        ]));

        // Create an empty batch with the schema
        let empty_batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(Vec::<&str>::new())),
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        Vec::<Option<Vec<Option<f32>>>>::new(),
                        self.embedding_dim as i32,
                    ),
                ),
                Arc::new(StringArray::from(Vec::<&str>::new())),
            ],
        )?;

        let batches =
            RecordBatchIterator::new(vec![empty_batch].into_iter().map(Ok), schema.clone());

        self.connection
            .create_table(table_name, Box::new(batches))
            .execute()
            .await
            .context("Failed to create table")?;

        // We don't create an index here since the table is empty
        // Index will be created when data is added

        Ok(())
    }

    async fn drop_table(&self, table_name: &str) -> Result<()> {
        self.connection
            .drop_table(table_name)
            .await
            .context("Failed to drop table")?;
        Ok(())
    }

    async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let tables = self.connection.table_names().execute().await?;
        Ok(tables.contains(&table_name.to_string()))
    }

    async fn add_embeddings(
        &self,
        table_name: &str,
        embeddings: Vec<DocumentEmbedding>,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        // Ensure the table exists
        let table_existed = self.table_exists(table_name).await?;
        if !table_existed {
            self.create_table(table_name).await?;
        }

        let table = self.connection.open_table(table_name).execute().await?;

        // Prepare the data for insertion
        let ids: Vec<&str> = embeddings.iter().map(|e| e.id.as_str()).collect();

        // Prepare the embeddings
        let embedding_vectors: Vec<Option<Vec<Option<f32>>>> = embeddings
            .iter()
            .map(|e| {
                // Validate that the embedding has the correct dimension
                if e.embedding.len() != self.embedding_dim {
                    None
                } else {
                    Some(e.embedding.iter().map(|&v| Some(v)).collect())
                }
            })
            .collect();

        // Serialize metadata to JSON strings
        let metadata_json: Vec<String> = embeddings
            .iter()
            .map(|e| serde_json::to_string(&e.metadata).unwrap_or_else(|_| "{}".to_string()))
            .collect();

        let metadata_refs: Vec<&str> = metadata_json.iter().map(|s| s.as_str()).collect();

        // Create schema and record batch
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.embedding_dim as i32,
                ),
                true,
            ),
            Field::new("metadata_json", DataType::Utf8, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(ids)),
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        embedding_vectors,
                        self.embedding_dim as i32,
                    ),
                ),
                Arc::new(StringArray::from(metadata_refs)),
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema.clone());

        // Add the data to the table
        table
            .add(Box::new(batches))
            .execute()
            .await
            .context("Failed to add embeddings to table")?;

        // If table was just created, create a vector index now that we have data
        if !table_existed {
            // Create a vector index on the embedding column
            // We can safely create an index now because we've just added data
            table
                .create_index(&["embedding"], lancedb::index::Index::Auto)
                .execute()
                .await
                .context("Failed to create vector index")?;
        }

        Ok(())
    }

    async fn delete_embeddings(&self, table_name: &str, ids: Vec<String>) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        // Ensure the table exists
        if !self.table_exists(table_name).await? {
            return Ok(());
        }

        let table = self.connection.open_table(table_name).execute().await?;

        // Build the SQL filter for deleting records with the specified IDs
        let ids_formatted: Vec<String> = ids.iter().map(|id| format!("'{}'", id)).collect();
        let filter = format!("id IN ({})", ids_formatted.join(","));

        // Execute the delete operation
        table
            .delete(filter.as_str())
            .await
            .context("Failed to delete embeddings")?;

        Ok(())
    }

    async fn similarity_search(
        &self,
        table_name: &str,
        query_vector: Vec<f32>,
        limit: usize,
        metadata_filter: Option<&str>,
    ) -> Result<Vec<(DocumentEmbedding, f32)>> {
        // Ensure the table exists
        if !self.table_exists(table_name).await? {
            return Ok(Vec::new());
        }

        // Verify query vector has the correct dimension
        if query_vector.len() != self.embedding_dim {
            return Err(anyhow::anyhow!(
                "Query vector dimension ({}) does not match the expected dimension ({})",
                query_vector.len(),
                self.embedding_dim
            ));
        }

        let table = self.connection.open_table(table_name).execute().await?;

        // Build and execute the query based on presence of filter
        let record_batches = if let Some(filter_str) = metadata_filter {
            // With filter: use only_if instead of filter
            // Clone the query_vector to avoid ownership issues
            let query_vec_clone = query_vector.clone();
            table
                .query()
                .only_if(filter_str)
                .nearest_to(query_vec_clone)?
                .limit(limit)
                .execute()
                .await?
                .try_collect::<Vec<_>>()
                .await?
        } else {
            // Without filter: directly do vector search
            table
                .query()
                .nearest_to(query_vector)?
                .limit(limit)
                .execute()
                .await?
                .try_collect::<Vec<_>>()
                .await?
        };

        // Parse results into DocumentEmbedding objects
        let mut results = Vec::new();
        for batch in record_batches {
            // Extract data from each row in the batch
            for row_idx in 0..batch.num_rows() {
                // Extract ID
                let id_array = batch
                    .column_by_name("id")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>());
                let id = match id_array {
                    Some(array) if !array.is_null(row_idx) => array.value(row_idx).to_string(),
                    _ => continue, // Skip if ID is null or column not found
                };

                // Extract embedding vector
                let embedding_array = batch
                    .column_by_name("embedding")
                    .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>());
                let embedding = match embedding_array {
                    Some(array) => {
                        if let Some(values) = array.values().as_any().downcast_ref::<Float32Array>()
                        {
                            // Calculate the start index in the flattened values array
                            let start_idx = row_idx * self.embedding_dim;
                            let end_idx = start_idx + self.embedding_dim;
                            // Extract the vector slice
                            if end_idx <= values.len() {
                                (start_idx..end_idx).map(|i| values.value(i)).collect()
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    }
                    None => Vec::new(),
                };

                // Extract metadata
                let metadata_array = batch
                    .column_by_name("metadata_json")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>());
                let metadata_json = match metadata_array {
                    Some(array) if !array.is_null(row_idx) => array.value(row_idx),
                    _ => "{}", // Default to empty JSON if not found
                };
                let metadata: HashMap<String, String> =
                    serde_json::from_str(metadata_json).unwrap_or_else(|_| HashMap::new());

                // Extract distance score
                let distance_array = batch
                    .column_by_name("_distance")
                    .and_then(|col| col.as_any().downcast_ref::<Float32Array>());
                let distance = match distance_array {
                    Some(array) if !array.is_null(row_idx) => array.value(row_idx),
                    _ => 0.0, // Default to 0.0 if not found
                };

                // Add to results
                results.push((
                    DocumentEmbedding {
                        id,
                        embedding,
                        metadata,
                    },
                    distance,
                ));
            }
        }

        Ok(results)
    }
}

// Helper for accessing columns by name
trait RecordBatchExt {
    fn column_by_name(&self, name: &str) -> Option<&ArrayRef>;
}

impl RecordBatchExt for RecordBatch {
    fn column_by_name(&self, name: &str) -> Option<&ArrayRef> {
        self.schema()
            .index_of(name)
            .ok()
            .map(|idx| self.column(idx))
    }
}

/// Create an embedding store with default configuration
pub async fn create_embedding_store(
    db_path: &str,
    embedding_dim: usize,
) -> Result<impl EmbeddingStore> {
    let config = EmbeddingStoreConfig {
        db_path: db_path.to_string(),
        embedding_dim,
    };

    LanceDBStore::connect(config).await
}
