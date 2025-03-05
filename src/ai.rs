// src/ai.rs
use crate::config::{self, Config};
use crate::embedding_config::EmbeddingManager;
use anyhow::{anyhow, Result};
use rust_bert::pipelines::sentence_embeddings::{
    SentenceEmbeddingsBuilder, SentenceEmbeddingsModel,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tch::Device;

/// AI module for generating and managing document embeddings
pub struct AI {
    /// The sentence embeddings model
    model: SentenceEmbeddingsModel,
    /// The embedding manager for storage
    embedding_manager: EmbeddingManager,
}

impl AI {
    /// Create a new AI instance with the given configuration
    pub async fn new(config: &Config) -> Result<Self> {
        // Initialize the embedding manager
        let embedding_manager = EmbeddingManager::new(config)
            .await
            .map_err(|e| anyhow!("Failed to initialize embedding manager: {}", e))?;

        // Initialize the model
        let model = Self::init_model()
            .map_err(|e| anyhow!("Failed to initialize embedding model: {}", e))?;

        Ok(Self {
            model,
            embedding_manager,
        })
    }

    /// Initialize the sentence embeddings model from the config directory
    fn init_model() -> Result<SentenceEmbeddingsModel> {
        // Model name - hardcoded since we only support one model
        let model_name = "all-MiniLM-L6-v2";

        // Get the config directory
        let config_dir = config::get_config_dir()
            .map_err(|e| anyhow!("Failed to get config directory: {}", e))?;

        // Path to the model in the config directory
        let model_path = config_dir.join(model_name);

        // Check if the model exists in the config directory
        if !model_path.exists() {
            return Err(anyhow!(
                "Model '{}' not found in config directory: {:?}. \
                Please download the model and place it in this directory.",
                model_name,
                model_path
            ));
        }

        // Load the model from the config directory
        let model = SentenceEmbeddingsBuilder::local(model_path.to_str().unwrap())
            .with_device(Device::cuda_if_available())
            .create_model()
            .map_err(|e| anyhow!("Failed to load model from config directory: {}", e))?;

        Ok(model)
    }

    /// Generate embeddings for a document
    pub fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // Encode the text to get embeddings
        let embeddings = self
            .model
            .encode(&[text])
            .map_err(|e| anyhow!("Failed to generate embeddings: {}", e))?;

        // Get the first embedding (for the single document)
        let embedding = embeddings[0].clone();

        Ok(embedding)
    }

    /// Store a document embedding in the database
    pub async fn store_document_embedding(
        &self,
        id: &str,
        text: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        // Generate the embedding
        let embedding = self.generate_embedding(text)?;

        // Create the document embedding
        let doc_embedding = crate::embeddings::DocumentEmbedding {
            id: id.to_string(),
            embedding,
            metadata,
        };

        // Store the embedding in the database
        let table_name = self.embedding_manager.get_table_name("documents");
        self.embedding_manager
            .store()
            .add_embeddings(&table_name, vec![doc_embedding])
            .await
            .map_err(|e| anyhow!("Failed to store document embedding: {}", e))?;

        Ok(())
    }

    /// Find similar documents by text
    pub async fn find_similar_documents(
        &self,
        text: &str,
        limit: usize,
        metadata_filter: Option<&str>,
    ) -> Result<Vec<(crate::embeddings::DocumentEmbedding, f32)>> {
        // Generate embedding for the query text
        let query_embedding = self.generate_embedding(text)?;

        // Find similar documents
        let table_name = self.embedding_manager.get_table_name("documents");
        let results = self
            .embedding_manager
            .store()
            .similarity_search(&table_name, query_embedding, limit, metadata_filter)
            .await
            .map_err(|e| anyhow!("Failed to find similar documents: {}", e))?;

        Ok(results)
    }

    /// Delete a document embedding by ID
    pub async fn delete_document_embedding(&self, id: &str) -> Result<()> {
        let table_name = self.embedding_manager.get_table_name("documents");
        self.embedding_manager
            .store()
            .delete_embeddings(&table_name, vec![id.to_string()])
            .await
            .map_err(|e| anyhow!("Failed to delete document embedding: {}", e))?;

        Ok(())
    }
}

// Helper function to convert a document into a metadata map
pub fn document_to_metadata(
    title: Option<&str>,
    author: Option<&str>,
    date: Option<&str>,
    tags: Option<Vec<&str>>,
    additional_fields: Option<HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    // Add basic metadata fields if provided
    if let Some(title) = title {
        metadata.insert("title".to_string(), title.to_string());
    }
    if let Some(author) = author {
        metadata.insert("author".to_string(), author.to_string());
    }
    if let Some(date) = date {
        metadata.insert("date".to_string(), date.to_string());
    }
    if let Some(tags) = tags {
        let tags_str = tags.join(",");
        metadata.insert("tags".to_string(), tags_str);
    }

    // Add any additional fields
    if let Some(additional) = additional_fields {
        metadata.extend(additional);
    }

    metadata
}

/// Helper function to check if the embedding model is available in the config directory
pub fn is_model_available() -> bool {
    if let Ok(config_dir) = config::get_config_dir() {
        let model_path = config_dir.join("all-MiniLM-L6-v2");
        model_path.exists()
    } else {
        false
    }
}

/// Helper function to get the path where the model should be installed
pub fn get_model_path() -> Result<PathBuf> {
    let config_dir =
        config::get_config_dir().map_err(|e| anyhow!("Failed to get config directory: {}", e))?;
    Ok(config_dir.join("all-MiniLM-L6-v2"))
}
