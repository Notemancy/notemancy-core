use crate::config::{self, Config};
use crate::embeddings::{create_embedding_store, EmbeddingStore};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;

/// Embedding configuration section for the config.yaml file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingConfig {
    /// The dimension of embeddings to store
    #[serde(default = "default_embedding_dim")]
    pub embedding_dim: usize,

    /// Directory name within the config directory for storing embeddings
    #[serde(default = "default_db_dir")]
    pub db_dir: String,

    /// Prefix for table names (helpful when using shared databases)
    #[serde(default = "default_table_prefix")]
    pub table_prefix: String,
}

// Default values for embedding configuration
fn default_embedding_dim() -> usize {
    384 // Common dimension for models like all-MiniLM-L6-v2
}

fn default_db_dir() -> String {
    "embeddings".to_string()
}

fn default_table_prefix() -> String {
    "notemancy_".to_string()
}

// Extension trait to add embedding support to AIConfig
pub trait AIConfigExt {
    fn get_embedding_config(&mut self) -> EmbeddingConfig;
}

// Implement extension trait for AIConfig
impl AIConfigExt for crate::config::AIConfig {
    fn get_embedding_config(&mut self) -> EmbeddingConfig {
        // Return a default embedding config
        EmbeddingConfig::default()
    }
}

/// Handles initialization of embedding storage and provides access to embedding operations
pub struct EmbeddingManager {
    store: Box<dyn EmbeddingStore + Send + Sync>,
    config: EmbeddingConfig,
}

impl EmbeddingManager {
    /// Creates a new EmbeddingManager with the specified configuration
    pub async fn new(_config: &Config) -> Result<Self, Box<dyn Error>> {
        // Using a default configuration since we can't modify the AIConfig
        let embedding_config = EmbeddingConfig {
            embedding_dim: default_embedding_dim(),
            db_dir: default_db_dir(),
            table_prefix: default_table_prefix(),
        };

        // Initialize the embedding store
        let store = Self::init_store(&embedding_config).await?;

        Ok(Self {
            store: Box::new(store),
            config: embedding_config,
        })
    }

    /// Initialize a local LanceDB store
    async fn init_store(
        config: &EmbeddingConfig,
    ) -> Result<impl EmbeddingStore + Send + Sync, Box<dyn Error>> {
        // Get the configuration directory
        let config_dir = config::get_config_dir()?;

        // Create the embeddings directory path
        let embeddings_dir = config_dir.join(&config.db_dir);

        // Create the directory if it doesn't exist
        if !embeddings_dir.exists() {
            std::fs::create_dir_all(&embeddings_dir)?;
        }

        // Create and return the embedding store
        let store =
            create_embedding_store(embeddings_dir.to_str().unwrap(), config.embedding_dim).await?;

        Ok(store)
    }

    /// Get the full table name with the configured prefix
    pub fn get_table_name(&self, base_name: &str) -> String {
        format!("{}{}", self.config.table_prefix, base_name)
    }

    /// Get a reference to the underlying embedding store
    pub fn store(&self) -> &dyn EmbeddingStore {
        self.store.as_ref()
    }

    /// Get the embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.config.embedding_dim
    }
}

// Helper function to get the embeddings directory path
pub fn get_embeddings_dir() -> Result<PathBuf, Box<dyn Error>> {
    // Use default embedding directory in config dir
    let config_dir = config::get_config_dir()?;
    Ok(config_dir.join(default_db_dir()))
}
