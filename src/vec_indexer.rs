// src/doc_indexer.rs

use crate::ai::AI;
use crate::db::Database;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use tokio::task;

/// Index markdown files in the workspace by generating embeddings and storing them in the database.
///
/// This function does the following:
/// 1. Wraps blocking database calls in a spawn_blocking closure.
/// 2. Filters the records to include only files with a ".md" or ".markdown" extension.
/// 3. Reads the file contents (also in a blocking closure) and builds a metadata map.
/// 4. Uses the AI instance to generate and store the embedding.
pub async fn index_markdown_files(ai: &AI) -> Result<()> {
    // Wrap blocking DB operations using spawn_blocking
    let file_records = task::spawn_blocking(|| {
        let db = Database::new().map_err(|e| anyhow!("Database initialization error: {}", e))?;
        db.get_file_tree()
            .map_err(|e| anyhow!("Error fetching file tree: {}", e))
    })
    .await
    .map_err(|e| anyhow!("Task join error: {}", e))??;

    // Process each file record.
    for record in file_records {
        if record.path.ends_with(".md") || record.path.ends_with(".markdown") {
            println!("Processing file: {}", record.path);

            // Read the file content in a blocking thread.
            // Clone the path for later use in the error message.
            let path = record.path.clone();
            let path_for_error = path.clone();
            let content = task::spawn_blocking(move || fs::read_to_string(&path))
                .await
                .map_err(|e| anyhow!("Task join error: {}", e))?
                .map_err(|e| anyhow!("Failed to read file {}: {}", path_for_error, e))?;

            // Build a metadata map using available record data.
            let mut metadata = HashMap::new();
            metadata.insert("physical_path".to_string(), record.path.clone());
            metadata.insert("virtual_path".to_string(), record.virtual_path.clone());
            metadata.insert("record_metadata".to_string(), record.metadata.clone());

            // Use the virtual path as the unique identifier.
            let id = record.virtual_path.as_str();

            // Generate and store the embedding.
            ai.store_document_embedding(id, &content, metadata).await?;
        }
    }
    Ok(())
}
