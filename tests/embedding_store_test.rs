// tests/embedding_store_test.rs
use notemancy_core::embeddings::{create_embedding_store, DocumentEmbedding, EmbeddingStore};
use std::collections::HashMap;
use tempfile::tempdir;

#[tokio::test]
async fn test_embedding_store_operations() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for the test database
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().to_str().unwrap();

    // Embedding dimension for testing
    let embedding_dim = 4;

    // Initialize the embedding store
    let store = create_embedding_store(db_path, embedding_dim).await?;

    // Test table name
    let table_name = "test_embeddings";

    // Step 1: Ensure the table doesn't exist initially
    assert!(!store.table_exists(table_name).await?);

    // Step 2: Create the table
    store.create_table(table_name).await?;

    // Step 3: Verify table exists
    assert!(store.table_exists(table_name).await?);

    // Step 4: Create test embeddings with metadata
    let embeddings = vec![
        create_test_embedding("doc1", vec![1.0, 0.0, 0.0, 0.0], "category", "science"),
        create_test_embedding("doc2", vec![0.0, 1.0, 0.0, 0.0], "category", "history"),
        create_test_embedding("doc3", vec![0.0, 0.0, 1.0, 0.0], "category", "science"),
        create_test_embedding("doc4", vec![0.0, 0.0, 0.0, 1.0], "category", "art"),
    ];

    // Step 5: Add embeddings to the store
    store.add_embeddings(table_name, embeddings).await?;

    // Step 6: Perform a similarity search
    let query_vector = vec![1.0, 0.2, 0.1, 0.0];
    let search_results = store
        .similarity_search(table_name, query_vector.clone(), 3, None)
        .await?;

    // Verify we got the expected number of results
    assert_eq!(search_results.len(), 3);

    // First result should be closest to our query (which is closest to doc1)
    assert_eq!(search_results[0].0.id, "doc1");

    // Step 7: Test search with metadata filter
    let filtered_results = store
        .similarity_search(
            table_name,
            query_vector.clone(),
            3,
            Some("metadata_json LIKE '%science%'"),
        )
        .await?;

    // Verify filter worked (should only return science documents)
    assert!(filtered_results.len() <= 2);
    for (embedding, _distance) in &filtered_results {
        let category = embedding.metadata.get("category").unwrap();
        assert_eq!(category, "science");
    }

    // Step 8: Delete one embedding
    store
        .delete_embeddings(table_name, vec!["doc1".to_string()])
        .await?;

    // Step 9: Check that the deleted embedding is gone
    let search_after_delete = store
        .similarity_search(table_name, query_vector, 4, None)
        .await?;

    // Should have 3 results now (doc1 is deleted)
    assert_eq!(search_after_delete.len(), 3);

    // Verify doc1 is not in the results
    for (embedding, _distance) in &search_after_delete {
        assert_ne!(embedding.id, "doc1");
    }

    // Step 10: Drop the table
    store.drop_table(table_name).await?;

    // Step 11: Verify the table is gone
    assert!(!store.table_exists(table_name).await?);

    println!("All embedding store tests passed!");
    Ok(())
}

// Helper function to create test embeddings
fn create_test_embedding(id: &str, vector: Vec<f32>, key: &str, value: &str) -> DocumentEmbedding {
    let mut metadata = HashMap::new();
    metadata.insert(key.to_string(), value.to_string());

    DocumentEmbedding {
        id: id.to_string(),
        embedding: vector,
        metadata,
    }
}
