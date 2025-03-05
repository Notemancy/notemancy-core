// tests/integration_vec_test.rs
mod common; // This imports tests/common/mod.rs
use anyhow::{anyhow, Result};
use notemancy_core::ai::AI;
use notemancy_core::config::load_config;
use notemancy_core::db::Database;
use notemancy_core::vec_indexer;
use std::fs;
use tokio::task;

#[tokio::test]
async fn test_vec_indexer_integration() -> Result<()> {
    // Set up the test environment inside a blocking task to ensure errors are Send+Sync.
    task::spawn_blocking(|| {
        common::setup_test_env(300).map_err(|e| anyhow!("setup_test_env error: {}", e))
    })
    .await??;

    // Load the configuration.
    let config = load_config().map_err(|e| anyhow!("load_config error: {}", e))?;
    println!("Loaded config: {:?}", config);

    // Create the AI instance (initializing the model and embedding manager).
    let ai = AI::new(&config).await?;

    // Run the vec_indexer to process markdown files (physical paths ending with .md or .markdown).
    vec_indexer::index_markdown_files(&ai).await?;

    // Query the database for file records.
    let db = Database::new().map_err(|e| anyhow!("Database::new error: {}", e))?;
    let file_records = db
        .get_file_tree()
        .map_err(|e| anyhow!("get_file_tree error: {}", e))?;

    // Find at least one markdown file record.
    let md_record = file_records
        .iter()
        .find(|r| r.path.ends_with(".md") || r.path.ends_with(".markdown"))
        .ok_or_else(|| anyhow!("No markdown file found in test environment"))?;

    println!("Found markdown file: {}", md_record.path);

    // Read the content of the markdown file.
    let content = fs::read_to_string(&md_record.path)
        .map_err(|e| anyhow!("Failed to read file {}: {}", md_record.path, e))?;

    // Test 1: Perform a similarity search using the file content.
    println!("Testing find_similar_documents API directly:");
    let similar = ai.find_similar_documents(&content, 10, None).await?;
    assert!(
        !similar.is_empty(),
        "Expected to find at least one similar document embedding"
    );
    println!(
        "Found {} similar embedding(s) for file: {}",
        similar.len(),
        md_record.path
    );

    // Test 2: Test the new find_related_documents function using physical path
    println!("\nTesting find_related_documents with physical path:");
    let related_by_physical_path =
        vec_indexer::find_related_documents(&ai, &md_record.path).await?;
    assert!(
        !related_by_physical_path.is_empty(),
        "Expected to find at least one related document by physical path"
    );
    println!(
        "Found {} related document(s) by physical path",
        related_by_physical_path.len()
    );

    // Print the top 3 most similar documents
    println!("Top related documents by physical path:");
    for (i, (path, score)) in related_by_physical_path.iter().take(3).enumerate() {
        println!("{}. {} (similarity: {:.2}%)", i + 1, path, score * 100.0);
    }

    // Test 3: Test find_related_documents using virtual path
    println!("\nTesting find_related_documents with virtual path:");
    let related_by_virtual_path =
        vec_indexer::find_related_documents(&ai, &md_record.virtual_path).await?;
    assert!(
        !related_by_virtual_path.is_empty(),
        "Expected to find at least one related document by virtual path"
    );
    println!(
        "Found {} related document(s) by virtual path",
        related_by_virtual_path.len()
    );

    // Print the top 3 most similar documents
    println!("Top related documents by virtual path:");
    for (i, (path, score)) in related_by_virtual_path.iter().take(3).enumerate() {
        println!("{}. {} (similarity: {:.2}%)", i + 1, path, score * 100.0);
    }

    // Test 4: Test with a custom threshold
    println!("\nTesting find_similar_documents with custom threshold (0.85):");
    let high_threshold_results =
        vec_indexer::find_similar_documents(&ai, &md_record.path, Some(0.85), None).await?;
    println!(
        "Found {} document(s) with similarity threshold 0.85",
        high_threshold_results.len()
    );

    // Test 5: Test with a limited number of results
    println!("\nTesting find_similar_documents with result limit (3):");
    let limited_results =
        vec_indexer::find_similar_documents(&ai, &md_record.path, None, Some(3)).await?;
    assert!(
        limited_results.len() <= 3,
        "Expected at most 3 results, but got {}",
        limited_results.len()
    );
    println!("Successfully limited results to {}", limited_results.len());

    Ok(())
}
