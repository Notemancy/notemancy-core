// tests/integration_test.rs
use lancedb::Result;
use notemancy_core::embeddings::{create_store, DocumentEmbedding, EmbeddingMetadata};
use rand::Rng;

#[tokio::test]
async fn test_random_vectors_index_and_search() -> Result<()> {
    // Create the embeddings store (this creates the table if needed)
    let store = create_store().await?;

    // Generate 100 random test vectors, each of dimension 768.
    let mut rng = rand::thread_rng();
    let embeddings: Vec<DocumentEmbedding> = (0..40)
        .map(|i| DocumentEmbedding {
            vector: (0..768).map(|_| rng.gen_range(0.0..1.0)).collect(),
            metadata: EmbeddingMetadata {
                id: i.to_string(),
                title: format!("Random Document {}", i),
                path: format!("/tmp/random_document_{}", i),
            },
        })
        .collect();

    // Add all embeddings to the store.
    store.add_embeddings(embeddings.clone()).await?;

    // Create an ANN index.
    // store.create_index().await?;

    // Use the first generated vector as a query.
    let query_vector = embeddings[0].vector.clone();
    let results = store.search(&query_vector, 10).await?;

    // Assert that we got some results.
    assert!(!results.is_empty(), "Search results should not be empty");

    // Assert that the document with id "0" is among the top results.
    let found = results.iter().any(|doc| doc.metadata.id == "0");
    assert!(
        found,
        "Expected to find document with id '0' in the search results"
    );

    Ok(())
}
