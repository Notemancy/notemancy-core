// tests/vectordb_integration.rs

use notemancy_core::vectordb::{Record, VectorDB};
use qdrant_client::qdrant::{DeleteCollectionBuilder, Distance};
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_ensure_collection_exists() {
    let collection_name = "test_collection";
    let dims = 100; // Dimensions for the vectors
    let distance = Distance::Cosine;

    // Connect to Qdrant (make sure a Qdrant instance is running at this address)
    let vectordb = VectorDB::new("http://localhost:6334").expect("Failed to connect to Qdrant");

    // Ensure the collection exists.
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

#[tokio::test]
async fn test_add_and_query_records() {
    let collection_name = "test_collection";
    let dims = 100;
    let distance = Distance::Cosine;

    // Connect to Qdrant.
    let vectordb = VectorDB::new("http://localhost:6334").expect("Failed to connect to Qdrant");

    // Ensure the test collection exists.
    vectordb
        .ensure_collection_exists(collection_name, dims, distance)
        .await
        .expect("Failed to ensure collection exists");
    sleep(Duration::from_secs(1)).await;

    // Prepare two fake records.
    let record1 = Record {
        id: 1,
        local_path: "local/path1.txt".to_string(),
        virtual_path: "virtual/path1.txt".to_string(),
        embedding: vec![0.1; dims],
    };

    let record2 = Record {
        id: 2,
        local_path: "local/path2.txt".to_string(),
        virtual_path: "virtual/path2.txt".to_string(),
        embedding: vec![0.2; dims],
    };

    // Add records to the collection.
    vectordb
        .add_records(collection_name, vec![record1, record2])
        .await
        .expect("Failed to add records");
    sleep(Duration::from_secs(1)).await;

    // Perform a vector query using a query vector similar to record1.
    let query_vector = vec![0.1; dims];
    let results = vectordb
        .query_by_vector(collection_name, query_vector)
        .await
        .expect("Failed to query by vector");

    // Check that at least one result has the expected local_path and virtual_path.
    let mut found = false;
    for res in results {
        if let (Some(local), Some(virtual_path)) = (res.local_path, res.virtual_path) {
            if local == "local/path1.txt" && virtual_path == "virtual/path1.txt" {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "Expected to find record with local 'local/path1.txt' and virtual 'virtual/path1.txt'"
    );

    // Cleanup: Delete the test collection.
    vectordb
        .client
        .delete_collection(DeleteCollectionBuilder::new(collection_name))
        .await
        .expect("Failed to delete test collection");
}
