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
        common::setup_test_env(1000).map_err(|e| anyhow!("setup_test_env error: {}", e))
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
        .into_iter()
        .find(|r| r.path.ends_with(".md") || r.path.ends_with(".markdown"))
        .ok_or_else(|| anyhow!("No markdown file found in test environment"))?;

    // Read the content of the markdown file.
    let content = fs::read_to_string(&md_record.path)
        .map_err(|e| anyhow!("Failed to read file {}: {}", md_record.path, e))?;

    // Perform a similarity search using the file content.
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

    Ok(())
}
