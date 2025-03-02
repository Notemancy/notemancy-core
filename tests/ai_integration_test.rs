// tests/ai_integration_test.rs
use notemancy_core::ai::AI;
use notemancy_core::config::Config;
use std::collections::HashMap;
use std::path::Path;

#[tokio::test]
async fn test_basic_embedding_flow() -> Result<(), Box<dyn std::error::Error>> {
    // Don't modify the environment variables - use the actual config directory
    // This ensures we use the model in the real config directory

    // Check if the model exists in the config directory
    let config_dir = notemancy_core::config::get_config_dir()?;
    let model_dir = config_dir.join("all-MiniLM-L12-v2");

    if !model_exists(&model_dir) {
        println!("=================================================================");
        println!("Model not found at: {:?}", model_dir);
        println!(
            "Please ensure you have the all-MiniLM-L12-v2 model files in your config directory."
        );
        println!(
            "For macOS, this should be at: ~/Library/Application Support/gnosis/all-MiniLM-L12-v2"
        );
        println!("=================================================================");
        return Ok(());
    }

    println!("Using model from: {:?}", model_dir);

    // Create a default config
    let config = Config::default();

    // Initialize the AI module
    println!("Initializing AI module...");
    let ai = AI::new(&config).await?;

    // Generate an embedding for a test document
    println!("Generating embedding...");
    let test_text = "This is a test document about artificial intelligence.";
    let embedding = ai.generate_embedding(test_text)?;

    // Verify embedding dimension (all-MiniLM-L12-v2 produces 384-dimensional vectors)
    assert_eq!(embedding.len(), 384);
    println!(
        "Successfully generated embedding with dimension: {}",
        embedding.len()
    );

    // Create metadata for the document
    let mut metadata = HashMap::new();
    metadata.insert("title".to_string(), "AI Test Document".to_string());
    metadata.insert("category".to_string(), "test".to_string());

    // Create a unique ID for the test document
    let doc_id = format!("test-doc-{}", chrono::Utc::now().timestamp());

    // Store the document embedding
    println!("Storing document embedding with ID: {}", doc_id);
    ai.store_document_embedding(&doc_id, test_text, metadata)
        .await?;

    // Retrieve the document using semantic search
    println!("Retrieving document via semantic search...");
    let search_query = "Tell me about AI.";
    let search_results = ai.find_similar_documents(search_query, 5, None).await?;

    // Verify we found our document
    assert!(
        !search_results.is_empty(),
        "Search results should not be empty"
    );
    println!("Found {} document(s)", search_results.len());

    // Check if our document is in the results
    let found_doc = search_results.iter().any(|(doc, _)| doc.id == doc_id);
    assert!(
        found_doc,
        "Our test document should be in the search results"
    );

    // Print the top result
    if !search_results.is_empty() {
        let (doc, score) = &search_results[0];
        println!(
            "Top result: ID={}, Title={}, Score={}",
            doc.id,
            doc.metadata.get("title").unwrap_or(&"Unknown".to_string()),
            score
        );
    }

    // Test filtering by metadata
    println!("Testing metadata filtering...");
    let filtered_results = ai
        .find_similar_documents(
            search_query,
            5,
            Some("metadata_json LIKE '%\"category\":\"test\"%'"),
        )
        .await?;

    assert!(
        !filtered_results.is_empty(),
        "Filtered results should not be empty"
    );
    println!(
        "Found {} document(s) with category 'test'",
        filtered_results.len()
    );

    // Delete the test document
    println!("Deleting test document...");
    ai.delete_document_embedding(&doc_id).await?;

    // Verify deletion
    let post_delete_results = ai.find_similar_documents(search_query, 5, None).await?;
    let doc_still_exists = post_delete_results.iter().any(|(doc, _)| doc.id == doc_id);
    assert!(!doc_still_exists, "Document should be deleted");
    println!("Document successfully deleted");

    println!("All tests passed successfully!");
    Ok(())
}

// Helper function to check if the model files exist
fn model_exists(model_dir: &Path) -> bool {
    model_dir.exists() && model_dir.join("config.json").exists()
}
