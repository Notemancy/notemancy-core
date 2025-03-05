// src/vec_indexer.rs

use crate::ai::AI;
use crate::db::Database;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::task;

const SIMILARITY_THRESHOLD: f32 = 0.1; // Similarity threshold (adjust as needed)
const MAX_RESULTS: usize = 20;
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_secs(2); // Update progress every 2 seconds

pub async fn index_markdown_files(ai: &AI) -> Result<()> {
    let start_time = Instant::now();

    // Get file records from database
    let db = Database::new().map_err(|e| anyhow!("Database initialization error: {}", e))?;
    let file_records = db
        .get_file_tree()
        .map_err(|e| anyhow!("Error fetching file tree: {}", e))?;

    // Track which physical paths we've indexed in this run to prevent duplicates
    let mut markdown_files = Vec::new();
    let mut indexed_paths = std::collections::HashSet::new();

    // First pass: collect all valid markdown files to process
    for record in file_records {
        if (record.path.ends_with(".md") || record.path.ends_with(".markdown"))
            && indexed_paths.insert(record.path.clone())
        {
            markdown_files.push(record);
        }
    }

    let total_files = markdown_files.len();
    println!(
        "Starting to index {} markdown files (SERIAL MODE)",
        total_files
    );

    if total_files == 0 {
        println!("No markdown files found to index");
        return Ok(());
    }

    // Counters for progress tracking
    let processed_count = Arc::new(AtomicUsize::new(0));
    let success_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));

    // Spawn a background task to periodically report progress
    let progress_processed = processed_count.clone();
    let progress_success = success_count.clone();
    let progress_error = error_count.clone();
    let progress_handle = tokio::spawn(async move {
        let mut last_update = Instant::now();
        let mut last_processed = 0;

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let current = progress_processed.load(Ordering::Relaxed);
            let successes = progress_success.load(Ordering::Relaxed);
            let errors = progress_error.load(Ordering::Relaxed);

            // Check if all files have been processed
            if current >= total_files {
                let elapsed = start_time.elapsed();
                let files_per_second = if elapsed.as_secs() > 0 {
                    total_files as f64 / elapsed.as_secs() as f64
                } else {
                    total_files as f64
                };

                println!("\rIndexing complete: {} files processed ({} succeeded, {} failed) in {:.1?} ({:.1} files/sec)",
                    total_files, successes, errors, elapsed, files_per_second);
                break;
            }

            // Update progress at regular intervals or if significant progress was made
            if last_update.elapsed() >= PROGRESS_UPDATE_INTERVAL
                || (current - last_processed) > total_files / 20
            {
                let elapsed = start_time.elapsed();
                let percent = (current as f64 / total_files as f64) * 100.0;
                let files_per_second = if elapsed.as_secs() > 0 {
                    current as f64 / elapsed.as_secs() as f64
                } else {
                    current as f64
                };

                let eta = if files_per_second > 0.0 {
                    let remaining_files = total_files - current;
                    let seconds_left = remaining_files as f64 / files_per_second;
                    format!("ETA: {:.0?}", Duration::from_secs_f64(seconds_left))
                } else {
                    "ETA: calculating...".to_string()
                };

                println!(
                    "\rIndexing progress: {}/{} files ({:.1}%) - {} - {:.1} files/sec",
                    current, total_files, percent, eta, files_per_second
                );

                last_update = Instant::now();
                last_processed = current;
            }
        }
    });

    // Process files serially
    println!("Starting serial file processing");

    // Process each file one at a time
    for record in markdown_files {
        let path = &record.path;

        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Failed to read file {}: {}", path, e);
                error_count.fetch_add(1, Ordering::Relaxed);
                processed_count.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        // Build metadata map
        let mut metadata = HashMap::new();
        metadata.insert("physical_path".to_string(), record.path.clone());
        metadata.insert("virtual_path".to_string(), record.virtual_path.clone());
        metadata.insert("record_metadata".to_string(), record.metadata.clone());

        // Use a more unique ID format
        let id = format!("{}|{}", record.virtual_path, record.path);

        // Delete any existing embedding first
        let _ = ai.delete_document_embedding(&id).await;

        // Generate and store embedding
        match ai.store_document_embedding(&id, &content, metadata).await {
            Ok(_) => {
                success_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("Error storing embedding for {}: {}", path, e);
                error_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Increment processed count
        processed_count.fetch_add(1, Ordering::Relaxed);
    }

    // Wait for the progress reporter to finish
    let _ = progress_handle.await;

    let elapsed = start_time.elapsed();
    println!("Serial indexing completed in {:.2?}", elapsed);

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
