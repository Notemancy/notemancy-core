use crate::config;
use anyhow::Result;
use rust_bert::pipelines::sentence_embeddings::{
    SentenceEmbeddingsBuilder, SentenceEmbeddingsModel,
};
use std::collections::HashMap;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;
use tch::Device;

use anndists::dist::DistDot;
use hnsw_rs::{api::AnnT, hnsw::Neighbour, prelude::*};

/// A struct that encapsulates the Sentence Embeddings model.
pub struct AIModel {
    model: SentenceEmbeddingsModel,
}

impl AIModel {
    /// Creates a new instance of AIModel from a local model path.
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model_path_str = model_path
            .as_ref()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid model path"))?;
        let model = SentenceEmbeddingsBuilder::local(model_path_str)
            .with_device(Device::cuda_if_available())
            .create_model()?;
        Ok(Self { model })
    }

    /// Generates an embedding for the provided document.
    pub fn embed_document(&self, document: &str) -> Result<Vec<f32>> {
        let mut embedding = self.model.encode(&[document])?;
        let vec = embedding
            .pop()
            .ok_or_else(|| anyhow::anyhow!("No embedding produced"))?;

        // Normalize the vector to unit length for better HNSW performance
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized = vec.iter().map(|x| x / norm).collect();
        Ok(normalized)
    }
}

pub struct AIStore {
    pub ai_model: AIModel,
    pub hnsw: Hnsw<'static, f32, DistDot>,
    pub mapping: HashMap<String, Vec<f32>>,
}

impl AIStore {
    pub fn new_with_params<P: AsRef<Path>>(
        model_path: P,
        initial_capacity: usize,
        max_connections: usize,
        ef_construction: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let ai_model = AIModel::new(model_path)?;
        let config_dir = config::get_config_dir()?;
        let mapping_path = config_dir.join("hnsw_mapping.json");

        // Initialize HNSW
        let mut hnsw = Hnsw::<f32, DistDot>::new(
            max_connections,
            initial_capacity,
            16,
            ef_construction,
            DistDot {},
        );
        hnsw.set_extend_candidates(true);

        // Load mapping if exists
        let mapping = if mapping_path.exists() {
            let file = File::open(&mapping_path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader)?
        } else {
            HashMap::new()
        };

        // Load HNSW index if exists
        if mapping_path.exists() {
            let basename = "hnsw_index";
            // The file_dump creates both .hnsw.graph and .hnsw.data files
            if config_dir.join(format!("{}.hnsw.graph", basename)).exists() {
                // Create the reloader and leak it so its lifetime becomes 'static.
                let reloader = Box::new(HnswIo::new(&config_dir, basename));
                let reloader_static: &'static mut HnswIo = Box::leak(reloader);
                hnsw = reloader_static.load_hnsw::<f32, DistDot>()?;
            }
        }

        Ok(Self {
            ai_model,
            hnsw,
            mapping,
        })
    }

    pub fn from_config() -> Result<Self, Box<dyn Error>> {
        let config = crate::config::load_config()?;
        let ai_config = config.ai.ok_or("No AI configuration found")?;
        let model_name = ai_config.model_name.ok_or("No AI model name configured")?;

        // Construct full model path from manifest directory
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let model_path = format!("{}/resources/{}", manifest_dir, model_name);

        // Fixed parameters that work with HNSW-rs
        let initial_capacity = 100;
        let max_connections = 16;
        let ef_construction = 100;

        Self::new_with_params(
            model_path,
            initial_capacity,
            max_connections,
            ef_construction,
        )
    }

    pub fn add_document(
        &mut self,
        document: &str,
        virtual_path: &str,
    ) -> Result<(), Box<dyn Error>> {
        // Compute the embedding
        let embedding = self.ai_model.embed_document(document)?;
        let new_id = self.hnsw.get_nb_point();

        // Insert into HNSW index
        self.hnsw.insert_data(&embedding, new_id);

        // Update the mapping
        self.mapping.insert(virtual_path.to_string(), embedding);

        // Save both index and mapping
        self.save()?;
        Ok(())
    }

    /// Add multiple documents in parallel
    pub fn add_documents(&mut self, documents: &[(&str, &str)]) -> Result<(), Box<dyn Error>> {
        for (content, path) in documents {
            self.add_document(content, path)?;
        }
        Ok(())
    }

    /// Searches for similar documents with optimized parameters
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<Neighbour>, Box<dyn Error>> {
        let embedding = self.ai_model.embed_document(query)?;

        // Optimized ef_search based on k
        let ef_search = if k <= 10 { 48 } else { 128 };

        // Search with optimized parameters
        Ok(self.hnsw.search_neighbours(&embedding, k, ef_search))
    }

    /// Parallel search for multiple queries
    pub fn parallel_search(&self, queries: &[&str], k: usize) -> Result<Vec<Vec<Neighbour>>> {
        let embeddings: Result<Vec<Vec<f32>>> = queries
            .iter()
            .map(|q| self.ai_model.embed_document(q))
            .collect();
        let embeddings = embeddings?;

        let ef_search = if k <= 10 { 48 } else { 128 };

        Ok(self
            .hnsw
            .parallel_search_neighbours(&embeddings, k, ef_search))
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let config_dir = config::get_config_dir()?;
        let mapping_path = config_dir.join("hnsw_mapping.json");

        // Save the HNSW index using AnnT trait
        let basename = "hnsw_index";
        self.hnsw.file_dump(&config_dir, basename)?;

        // Save the mapping
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(mapping_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.mapping)?;

        Ok(())
    }

    /// Get the number of documents in the store
    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    /// Returns a reference to the mapping.
    pub fn get_mapping(&self) -> &HashMap<String, Vec<f32>> {
        &self.mapping
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_env() -> Result<(), Box<dyn Error>> {
        let config_dir = config::get_config_dir()?;
        std::fs::create_dir_all(&config_dir)?;

        // Clean up any existing files from previous runs
        let mapping_file = config_dir.join("hnsw_mapping.json");
        if mapping_file.exists() {
            std::fs::remove_file(&mapping_file)?;
        }
        let index_graph = config_dir.join("hnsw_index.hnsw.graph");
        if index_graph.exists() {
            std::fs::remove_file(&index_graph)?;
        }
        let index_data = config_dir.join("hnsw_index.hnsw.data");
        if index_data.exists() {
            std::fs::remove_file(&index_data)?;
        }

        let config_content = r#"
general:
  indicator: "notesy"
vaults:
  main:
    default: true
    paths:
      - "path/to/test_vault/main"
ai:
  model_name: "all-MiniLM-L12-v2"
  initial_capacity: 100
  ef_construction: 100
  max_connections: 16
"#;
        let config_file = config_dir.join("config.yaml");
        std::fs::write(&config_file, config_content)?;
        Ok(())
    }

    #[test]
    fn test_ai_store_with_test_env() -> Result<(), Box<dyn Error>> {
        setup_test_env()?;

        // First instance: add documents
        {
            let mut ai_store = AIStore::from_config()?;
            assert_eq!(ai_store.len(), 0, "Should start empty");

            let test_docs = vec![
                ("This is a test document about programming", "doc1.md"),
                ("Another document about testing software", "doc2.md"),
                ("A third document about rust programming", "doc3.md"),
            ];

            ai_store.add_documents(&test_docs)?;
            assert_eq!(ai_store.len(), 3, "Should have 3 documents after adding");
            ai_store.save()?;
        }

        // Second instance: verify loading
        {
            let loaded_store = AIStore::from_config()?;
            assert_eq!(
                loaded_store.len(),
                3,
                "Should have 3 documents after loading"
            );
            assert_eq!(
                loaded_store.hnsw.get_nb_point(),
                3,
                "HNSW should have 3 points"
            );

            // Test search functionality
            let results = loaded_store.search("programming in rust", 2)?;
            assert_eq!(results.len(), 2, "Should find 2 nearest neighbors");
        }

        Ok(())
    }
}
