use crate::dbapi;
use arrow_array::types::Float32Type;

use arrow_array::{ArrayRef, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use chrono::Utc; // <-- Add this at the top of your file.
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use lancedb::index::scalar::FullTextSearchQuery;
use lancedb::{
    connect,
    index::scalar::FtsIndexBuilder,
    index::vector::IvfPqIndexBuilder,
    index::Index,
    query::{ExecutableQuery, QueryBase},
    Connection, DistanceType, Error, Result, Table,
};

use crate::confapi;

const EMBEDDING_DIM: usize = 384;
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

/// A document embedding with its metadata and full text content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEmbedding {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Metadata for the embedding.
    pub metadata: EmbeddingMetadata,
    /// Full text content for the record.
    pub content: String,
}

/// Manager for storing and retrieving embeddings.
pub struct EmbeddingsStore {
    connection: Connection,
    table: Option<Table>,
}

impl EmbeddingsStore {
    /// Create a new embeddings store.
    ///
    /// This function uses the new confapi module to determine the database directory.
    pub async fn new() -> Result<Self> {
        let config_dir = confapi::get_config_dir();
        let embeddings_dir = config_dir.join("embeddings");
        if !embeddings_dir.exists() {
            std::fs::create_dir_all(&embeddings_dir).map_err(|e| Error::Other {
                message: format!("Failed to create embeddings directory: {}", e),
                source: None,
            })?;
        }
        let connection = connect(&embeddings_dir.to_string_lossy()).execute().await?;
        let mut store = Self {
            connection,
            table: None,
        };

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

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("title", DataType::Utf8, true),
            Field::new("path", DataType::Utf8, true),
            Field::new("content", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBEDDING_DIM as i32,
                ),
                true,
            ),
        ]));

        let empty_batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(Vec::<&str>::new())), // id
                Arc::new(StringArray::from(Vec::<&str>::new())), // title
                Arc::new(StringArray::from(Vec::<&str>::new())), // path
                Arc::new(StringArray::from(Vec::<&str>::new())), // content
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        Vec::<Option<Vec<Option<f32>>>>::new(),
                        EMBEDDING_DIM as i32,
                    ),
                ),
            ],
        )?;

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

    /// Retrieves an existing embedding by its file path.
    pub async fn get_embedding_by_path(&self, path: &str) -> Result<Option<DocumentEmbedding>> {
        // Use the full-text search API on the "path" field.
        // Note: This is a heuristic; ensure that your "path" values are unique enough.
        let embeddings = self.search_text(path, 10).await?;
        for emb in embeddings {
            if emb.metadata.path == path {
                return Ok(Some(emb));
            }
        }
        Ok(None)
    }

    pub async fn delete_embedding_by_path(&self, path: &str) -> Result<()> {
        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;
        let predicate = format!("path = '{}'", path);
        table.delete(&predicate).await?;
        Ok(())
    }

    pub async fn add_embedding(&self, embedding: DocumentEmbedding) -> Result<()> {
        // Check if the record already exists in SQLite.
        if dbapi::record_exists(embedding.metadata.path.as_str()).map_err(|e| {
            lancedb::Error::Other {
                message: format!("SQLite error: {}", e),
                source: None,
            }
        })? {
            println!(
                "Record already exists in SQLite, skipping insertion: {}",
                embedding.metadata.path
            );
            return Ok(());
        }

        // Ensure the embedding vector has the expected dimension.
        if embedding.vector.len() != EMBEDDING_DIM {
            return Err(Error::InvalidInput {
                message: format!(
                    "Embedding vector dimension {} does not match expected {}",
                    embedding.vector.len(),
                    EMBEDDING_DIM
                ),
            });
        }

        // Prepare the columns for the record batch.
        let id = Arc::new(StringArray::from(vec![embedding.metadata.id.clone()]));
        let title = Arc::new(StringArray::from(vec![embedding.metadata.title.clone()]));
        let path = Arc::new(StringArray::from(vec![embedding.metadata.path.clone()]));
        let content = Arc::new(StringArray::from(vec![embedding.content]));
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
            ("content", content as ArrayRef),
            ("vector", vector as ArrayRef),
        ])?;

        let schema_ref: SchemaRef = batch.schema();
        let iter = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema_ref);
        self.table
            .as_ref()
            .ok_or(Error::Other {
                message: "Table not initialized".to_string(),
                source: None,
            })?
            .add(Box::new(iter))
            .execute()
            .await?;
        println!("Added record to LanceDB: {}", embedding.metadata.path);

        // Add the record to SQLite.
        let timestamp = Utc::now().to_rfc3339();
        let record = dbapi::Record {
            lpath: embedding.metadata.path,
            title: embedding.metadata.title,
            timestamp,
            // Adjust vpath as needed. Here we use an empty string if not applicable.
            vpath: "".to_string(),
            project: None,
        };
        match dbapi::add_record(&record) {
            Ok(status) => match status {
                dbapi::AddRecordStatus::Inserted => {
                    println!("Inserted record into SQLite DB: {}", record.lpath)
                }
                dbapi::AddRecordStatus::AlreadyExists => {
                    println!("Record already exists in SQLite DB: {}", record.lpath)
                }
            },
            Err(e) => eprintln!("Failed to insert record into SQLite DB: {}", e),
        }

        Ok(())
    }

    /// Add multiple document embeddings to the store.
    pub async fn add_embeddings(&self, embeddings: Vec<DocumentEmbedding>) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let _table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;

        for embedding in embeddings {
            self.add_embedding(embedding).await?;
        }
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

    /// Create a full text search (FTS) index on the "content" field.
    pub async fn create_text_index(&self) -> Result<()> {
        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;
        table
            .create_index(&["content"], Index::FTS(FtsIndexBuilder::default()))
            .execute()
            .await?;
        Ok(())
    }

    /// Search for similar embeddings based on vector similarity.
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

                let content = batch
                    .column_by_name("content")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| Error::Other {
                        message: "Failed to get content column".to_string(),
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
                    content,
                });
            }
        }
        Ok(embeddings)
    }

    /// Search for records using full text search on the "content" field.
    pub async fn search_text(&self, query: &str, limit: usize) -> Result<Vec<DocumentEmbedding>> {
        let table = self.table.as_ref().ok_or(Error::Other {
            message: "Table not initialized".to_string(),
            source: None,
        })?;

        let mut results = table
            .query()
            .full_text_search(FullTextSearchQuery::new(query.to_owned()))
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

                let content = batch
                    .column_by_name("content")
                    .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| Error::Other {
                        message: "Failed to get content column".to_string(),
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
                    content,
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
