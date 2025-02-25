use notemancy_core::vectordb::VectorDB;
use qdrant_client::qdrant::Distance;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_ensure_collection_exists() {
    let collection_name = "test_collection";
    let dims = 100; // Dimensions for the vectors
    let distance = Distance::Cosine;

    // Connect to Qdrant (make sure a Qdrant instance is running at this address)
    let vectordb = VectorDB::new("http://localhost:6334").expect("Failed to connect to Qdrant");

    // Ensure the collection exists.
    // If it exists with a healthy status, nothing happens.
    // If it doesn't exist, it is created.
    vectordb
        .ensure_collection_exists(collection_name, dims, distance)
        .await
        .expect("Failed to ensure collection exists");

    // Optionally wait a short time to ensure that any recent changes are applied.
    sleep(Duration::from_secs(1)).await;

    // Retrieve the collection info.
    let info_response = vectordb
        .collection_info(collection_name)
        .await
        .expect("Failed to retrieve collection info");
    let info = info_response
        .result
        .expect("Collection info result is missing");

    // Check that the collection status is healthy: 0 (green) or 1 (yellow)
    assert!(
        info.status == 0 || info.status == 1,
        "Expected collection status to be green (0) or yellow (1), but got {}",
        info.status
    );
}
