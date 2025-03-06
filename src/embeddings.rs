// embeddings.rs
use std::path::PathBuf;
use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{ArrayRef, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};

use lancedb::{
    connect,
    index::vector::IvfPqIndexBuilder,
    index::Index,
    query::{ExecutableQuery, QueryBase},
    Connection, DistanceType, Error, Result, Table,
};

use crate::config;

const EMBEDDING_DIM: usize = 768;
const TABLE_NAME: &str = "embeddings";

/// Metadata associated with an embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    /// Unique identifier for the embedding.
    pub id: String,
    /// Title or name of the document.
    pub title: String,
    /// Filesystem path or URI to the source document.
    pub path: String,
}

/// A document embedding with its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEmbedding {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Metadata for the embedding.
    pub metadata: EmbeddingMetadata,
}

/// Manager for storing and retrieving embeddings.
pub struct EmbeddingsStore {
    connection: Connection,
    table: Option<Table>,
}

impl EmbeddingsStore {
    /// Create a new embeddings store.
    ///
    /// This function uses the config module to determine the database directory.
    pub async fn new() -> Result<Self> {
        // Get the config directory or default to "./data"
        let config_dir = config::get_config_dir().unwrap_or_else(|_| PathBuf::from("./data"));
        // Create embeddings directory under the config directory
        let embeddings_dir = config_dir.join("embeddings");
        if !embeddings_dir.exists() {
            std::fs::create_dir_all(&embeddings_dir).map_err(|e| Error::Other {
                message: format!("Failed to create embeddings directory: {}", e),
                source: None,
            })?;
        }

        // Connect to the LanceDB instance at the embeddings directory.
        let connection = connect(&embeddings_dir.to_string_lossy()).execute().await?;
        let mut store = Self {
            connection,
            table: None,
        };

        // If the table exists, open it.
        let tables = store.connection.table_names().execute().await?;
        if tables.contains(&TABLE_NAME.to_string()) {
            store.table = Some(store.connection.open_table(TABLE_NAME).execute().await?);
        }
        Ok(store)
    }

    /// Check if the embeddings table exists.
    pub async fn table_exists(&self) -> Result<bool> {
        let tables = self.connection.table_names().execute().await?;
        Ok(tables.contains(&TABLE_NAME.to_string()))
    }

    /// Create a new table with the fixed schema.
    pub async fn create_table(&mut self) -> Result<()> {
        if self.table_exists().await? {
            self.table = Some(self.connection.open_table(TABLE_NAME).execute().await?);
            return Ok(());
        }

        // Define the schema with a hard-coded embedding dimension.
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("title", DataType::Utf8, true),
            Field::new("path", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM as i32,
                ),
                true,
            ),
        ]));

        // Create an empty record batch.
        let empty_batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(Vec::<&str>::new())),
                Arc::new(StringArray::from(Vec::<&str>::new())),
                Arc::new(StringArray::from(Vec::<&str>::new())),
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        Vec::<Option<Vec<Option<f32>>>>::new(),
                        EMBEDDING_DIM as i32,
                    ),
                ),
            ],
        )?;

        // Create a batch iterator and create the table.
        let batches =
            RecordBatchIterator::new(vec![empty_batch].into_iter().map(Ok), schema.clone());
        let table = self
            .connection
            .create_table(TABLE_NAME, Box::new(batches))
            .execute()
            .await?;

        self.table = Some(table);
        Ok(())
    }

    /// Add a single document embedding to the store.
    pub async fn add_embedding(&self, embedding: DocumentEmbedding) -> Result<()> {
        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;

        if embedding.vector.len() != EMBEDDING_DIM {
            return Err(Error::InvalidInput {
                message: format!(
                    "Embedding vector dimension {} does not match expected {}",
                    embedding.vector.len(),
                    EMBEDDING_DIM
                ),
            });
        }

        let id = Arc::new(StringArray::from(vec![embedding.metadata.id]));
        let title = Arc::new(StringArray::from(vec![embedding.metadata.title]));
        let path = Arc::new(StringArray::from(vec![embedding.metadata.path]));
        let vector = Arc::new(
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                vec![Some(
                    embedding.vector.into_iter().map(Some).collect::<Vec<_>>(),
                )],
                EMBEDDING_DIM as i32,
            ),
        );

        let batch = RecordBatch::try_from_iter(vec![
            ("id", id as ArrayRef),
            ("title", title as ArrayRef),
            ("path", path as ArrayRef),
            ("vector", vector as ArrayRef),
        ])?;

        let schema_ref: SchemaRef = batch.schema();
        let iter = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema_ref);
        table.add(Box::new(iter)).execute().await?;
        Ok(())
    }

    /// Add multiple document embeddings to the store.
    pub async fn add_embeddings(&self, embeddings: Vec<DocumentEmbedding>) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;

        // Validate that all vectors have the correct dimension.
        for emb in &embeddings {
            if emb.vector.len() != EMBEDDING_DIM {
                return Err(Error::InvalidInput {
                    message: format!(
                        "Embedding vector dimension {} does not match expected {}",
                        emb.vector.len(),
                        EMBEDDING_DIM
                    ),
                });
            }
        }

        let ids: Vec<&str> = embeddings.iter().map(|e| e.metadata.id.as_str()).collect();
        let titles: Vec<&str> = embeddings
            .iter()
            .map(|e| e.metadata.title.as_str())
            .collect();
        let paths: Vec<&str> = embeddings
            .iter()
            .map(|e| e.metadata.path.as_str())
            .collect();
        let vectors: Vec<Option<Vec<Option<f32>>>> = embeddings
            .iter()
            .map(|e| Some(e.vector.iter().map(|&v| Some(v)).collect()))
            .collect();

        let id_array = Arc::new(StringArray::from(ids));
        let title_array = Arc::new(StringArray::from(titles));
        let path_array = Arc::new(StringArray::from(paths));
        let vector_array = Arc::new(
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                vectors,
                EMBEDDING_DIM as i32,
            ),
        );

        let batch = RecordBatch::try_from_iter(vec![
            ("id", id_array as ArrayRef),
            ("title", title_array as ArrayRef),
            ("path", path_array as ArrayRef),
            ("vector", vector_array as ArrayRef),
        ])?;

        let schema_ref: SchemaRef = batch.schema();
        let iter = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema_ref);
        table.add(Box::new(iter)).execute().await?;
        Ok(())
    }

    /// Create an approximate nearest neighbor (ANN) index for faster vector search.
    pub async fn create_index(&self) -> Result<()> {
        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;

        table
            .create_index(
                &["vector"],
                Index::IvfPq(
                    IvfPqIndexBuilder::default()
                        .distance_type(DistanceType::Cosine)
                        .num_partitions(5)
                        .num_sub_vectors(16),
                ),
            )
            .execute()
            .await?;
        Ok(())
    }

    /// Search for similar embeddings.
    pub async fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<DocumentEmbedding>> {
        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;

        if query_vector.len() != EMBEDDING_DIM {
            return Err(Error::InvalidInput {
                message: format!(
                    "Query vector dimension {} does not match expected {}",
                    query_vector.len(),
                    EMBEDDING_DIM
                ),
            });
        }

        let mut results = table
            .vector_search(query_vector)?
            .distance_type(DistanceType::Cosine)
            .limit(limit)
            .execute()
            .await?;

        let mut embeddings = Vec::new();
        while let Some(batch) = results.try_next().await? {
            for row_idx in 0..batch.num_rows() {
                let id = batch
                    .column_by_name("id")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| Error::Other {
                        message: "Failed to get id column".to_string(),
                        source: None,
                    })?
                    .value(row_idx)
                    .to_string();

                let title = batch
                    .column_by_name("title")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| Error::Other {
                        message: "Failed to get title column".to_string(),
                        source: None,
                    })?
                    .value(row_idx)
                    .to_string();

                let path = batch
                    .column_by_name("path")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| Error::Other {
                        message: "Failed to get path column".to_string(),
                        source: None,
                    })?
                    .value(row_idx)
                    .to_string();

                let vector_col = batch
                    .column_by_name("vector")
                    .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>())
                    .ok_or_else(|| Error::Other {
                        message: "Failed to get vector column".to_string(),
                        source: None,
                    })?;

                // Reconstruct the embedding vector.
                let vector_values: Vec<f32> = (0..EMBEDDING_DIM)
                    .map(|i| {
                        let list_value = vector_col.value(row_idx);
                        if i < list_value.len() {
                            if let Some(float_array) = list_value
                                .as_any()
                                .downcast_ref::<arrow_array::Float32Array>()
                            {
                                return float_array.value(i);
                            }
                        }
                        0.0
                    })
                    .collect();

                embeddings.push(DocumentEmbedding {
                    vector: vector_values,
                    metadata: EmbeddingMetadata { id, title, path },
                });
            }
        }
        Ok(embeddings)
    }
}

/// Helper function to create a new embeddings store with a table.
pub async fn create_store() -> Result<EmbeddingsStore> {
    let mut store = EmbeddingsStore::new().await?;
    store.create_table().await?;
    Ok(store)
}
