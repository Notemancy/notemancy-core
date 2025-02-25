// tests/vectordb_integration.rs

use notemancy_core::vectordb::{Record, VectorDB};
use qdrant_client::qdrant::{DeleteCollectionBuilder, Distance};
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_ensure_collection_exists() {
    let collection_name = "test_collection";
    let dims = 100;
    let distance = Distance::Cosine;

    // Connect to Qdrant (ensure a Qdrant instance is running at this address)
    let vectordb = VectorDB::new("http://localhost:6334").expect("Failed to connect to Qdrant");

    // Ensure the collection exists.
    vectordb
        .ensure_collection_exists(collection_name, dims, distance)
        .await
        .expect("Failed to ensure collection exists");

    // Wait for propagation.
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
}

#[tokio::test]
async fn test_delete_points_by_field() {
    let collection_name = "test_collection";
    let dims = 100;
    let distance = Distance::Cosine;

    let vectordb = VectorDB::new("http://localhost:6334").expect("Failed to connect to Qdrant");

    // Ensure the test collection exists.
    vectordb
        .ensure_collection_exists(collection_name, dims, distance)
        .await
        .expect("Failed to ensure collection exists");
    sleep(Duration::from_secs(1)).await;

    // Add three fake records (one of them will be deleted).
    let record1 = Record {
        id: 3,
        local_path: "delete_me.txt".to_string(),
        virtual_path: "virtual/delete_me.txt".to_string(),
        embedding: vec![0.3; dims],
    };
    let record2 = Record {
        id: 4,
        local_path: "keep_me.txt".to_string(),
        virtual_path: "virtual/keep_me.txt".to_string(),
        embedding: vec![0.4; dims],
    };
    let record3 = Record {
        id: 5,
        local_path: "delete_me.txt".to_string(),
        virtual_path: "virtual/delete_me.txt".to_string(),
        embedding: vec![0.5; dims],
    };

    vectordb
        .add_records(collection_name, vec![record1, record2, record3])
        .await
        .expect("Failed to add records");
    sleep(Duration::from_secs(1)).await;

    // Query to verify records are present.
    let query_vector = vec![0.3; dims];
    let results_before = vectordb
        .query_by_vector(collection_name, query_vector.clone())
        .await
        .expect("Failed to query before deletion");

    let found_before = results_before.iter().any(|res| {
        res.local_path.as_deref() == Some("delete_me.txt")
            && res.virtual_path.as_deref() == Some("virtual/delete_me.txt")
    });
    assert!(
        found_before,
        "Expected to find records with local_path 'delete_me.txt' before deletion"
    );

    // Delete points where local_path is "delete_me.txt".
    vectordb
        .delete_points_by_field(collection_name, "local_path", "delete_me.txt".to_string())
        .await
        .expect("Failed to delete points by field");
    sleep(Duration::from_secs(1)).await;

    // Query again to verify deletion.
    let results_after = vectordb
        .query_by_vector(collection_name, query_vector)
        .await
        .expect("Failed to query after deletion");

    let found_after = results_after.iter().any(|res| {
        res.local_path.as_deref() == Some("delete_me.txt")
            && res.virtual_path.as_deref() == Some("virtual/delete_me.txt")
    });
    assert!(
        !found_after,
        "Expected no records with local_path 'delete_me.txt' after deletion"
    );
}

#[tokio::test]
async fn cleanup_test_collection() {
    let collection_name = "test_collection";
    let vectordb = VectorDB::new("http://localhost:6334").expect("Failed to connect to Qdrant");
    // Delete the test collection.
    vectordb
        .client
        .delete_collection(DeleteCollectionBuilder::new(collection_name))
        .await
        .expect("Failed to delete test collection");
}
