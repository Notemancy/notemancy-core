// src/doc_indexer.rs

use crate::ai::AI;
use crate::db::Database;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tokio::task;

const SIMILARITY_THRESHOLD: f32 = 0.1; // Similarity threshold (adjust as needed)
const MAX_RESULTS: usize = 20;

pub async fn index_markdown_files(ai: &AI) -> Result<()> {
    // Wrap blocking DB operations using spawn_blocking
    let file_records = task::spawn_blocking(|| {
        let db = Database::new().map_err(|e| anyhow!("Database initialization error: {}", e))?;
        db.get_file_tree()
            .map_err(|e| anyhow!("Error fetching file tree: {}", e))
    })
    .await
    .map_err(|e| anyhow!("Task join error: {}", e))??;

    // Track which physical paths we've indexed in this run to prevent duplicates
    let mut indexed_paths = std::collections::HashSet::new();

    // Process each file record
    for record in file_records {
        if record.path.ends_with(".md") || record.path.ends_with(".markdown") {
            // Skip if we've already indexed this physical path in this run
            if !indexed_paths.insert(record.path.clone()) {
                continue;
            }

            // Read the file content in a blocking thread
            let path = record.path.clone();
            let path_for_error = path.clone();
            let content = task::spawn_blocking(move || fs::read_to_string(&path))
                .await
                .map_err(|e| anyhow!("Task join error: {}", e))?
                .map_err(|e| anyhow!("Failed to read file {}: {}", path_for_error, e))?;

            // Build a metadata map using available record data
            let mut metadata = HashMap::new();
            metadata.insert("physical_path".to_string(), record.path.clone());
            metadata.insert("virtual_path".to_string(), record.virtual_path.clone());
            metadata.insert("record_metadata".to_string(), record.metadata.clone());

            // Use a more unique ID format that includes both virtual path and physical path
            // This helps ensure we don't get ID collisions while maintaining uniqueness for physical paths
            let id = format!("{}|{}", record.virtual_path, record.path);

            // Try to delete any existing embedding with this ID first (ignore errors if not found)
            let _ = ai.delete_document_embedding(&id).await;

            // Generate and store the embedding
            ai.store_document_embedding(&id, &content, metadata).await?;
        }
    }
    Ok(())
}

/// Search for semantically similar documents based on a given document path.
///
/// This function will:
/// 1. Determine if the path is a virtual path or physical path
/// 2. Fetch the document content
/// 3. Generate an embedding for the document
/// 4. Find similar documents using vector similarity search
///
/// # Arguments
/// * `ai` - Reference to the AI instance for embedding generation
/// * `path` - Either a virtual path or physical path to a document
/// * `threshold` - Optional similarity threshold (0.0 to 1.0), defaults to 0.75
/// * `max_results` - Optional maximum number of results, defaults to 20
///
/// # Returns
/// * A vector of tuples containing file paths and similarity scores
pub async fn find_similar_documents(
    ai: &AI,
    path: &str,
    threshold: Option<f32>,
    max_results: Option<usize>,
) -> Result<Vec<(String, f32)>> {
    // Set default values if not provided
    let threshold = threshold.unwrap_or(SIMILARITY_THRESHOLD);
    let max_results = max_results.unwrap_or(MAX_RESULTS);

    // Determine if this is a virtual path or physical path
    let is_virtual_path = !Path::new(path).exists();

    // Get the document content based on path type
    let content = if is_virtual_path {
        // For virtual paths, find the corresponding physical path from the database
        let virtual_path = path.to_string();

        // We need to make sure the content is returned from this block
        task::spawn_blocking(move || {
            let db =
                Database::new().map_err(|e| anyhow!("Database initialization error: {}", e))?;

            let file_records = db
                .get_file_tree()
                .map_err(|e| anyhow!("Error fetching file tree: {}", e))?;

            // Find the record with matching virtual path
            let record = file_records
                .iter()
                .find(|r| r.virtual_path == virtual_path)
                .ok_or_else(|| anyhow!("No document found with virtual path: {}", virtual_path))?;

            // Read the file content
            fs::read_to_string(&record.path)
                .map_err(|e| anyhow!("Failed to read file {}: {}", record.path, e))
        })
        .await?
        .map_err(|e| anyhow!("Failed to process virtual path: {}", e))?
    } else {
        // For physical paths, directly read the file
        let physical_path = path.to_string();
        task::spawn_blocking(move || {
            fs::read_to_string(&physical_path)
                .map_err(|e| anyhow!("Failed to read file {}: {}", physical_path, e))
        })
        .await?
        .map_err(|e| anyhow!("Failed to read file: {}", e))?
    };

    // Use the AI to find similar documents
    let similar_docs = ai
        .find_similar_documents(&content, max_results, None)
        .await
        .map_err(|e| anyhow!("Failed to find similar documents: {}", e))?;

    // Process the results, filtering by threshold and converting to return format
    let mut results = Vec::new();

    for (doc, similarity_score) in similar_docs {
        // Convert similarity score: some embeddings use distance metrics where
        // lower is better, so we might need to convert to a similarity score
        // where higher means more similar

        // If the score is a distance (lower is better), convert to similarity
        // Adjust this formula based on the specific embedding model used
        let similarity = 1.0 - similarity_score;

        // Filter by threshold
        if similarity >= threshold {
            // Get the physical path from metadata
            if let Some(path) = doc.metadata.get("physical_path") {
                results.push((path.clone(), similarity));
            }
        }
    }

    // Sort by similarity (highest first)
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(results)
}

/// Find documents related to a specific document by path
///
/// Convenience wrapper around find_similar_documents with default parameters
///
/// # Arguments
/// * `ai` - Reference to the AI instance
/// * `path` - Path to the document (virtual or physical)
///
/// # Returns
/// * A vector of tuples containing file paths and similarity scores
pub async fn find_related_documents(ai: &AI, path: &str) -> Result<Vec<(String, f32)>> {
    find_similar_documents(ai, path, None, None).await
}
