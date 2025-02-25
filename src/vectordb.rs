// src/vectordb.rs

use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, GetCollectionInfoResponse, VectorParamsBuilder,
};
use qdrant_client::{Qdrant, QdrantError};

/// A simple wrapper around a connected Qdrant client instance.
pub struct VectorDB {
    /// Expose the underlying Qdrant client for additional operations.
    pub client: Qdrant,
}

impl VectorDB {
    /// Connect to a Qdrant instance and return a new `VectorDB`.
    ///
    /// # Arguments
    ///
    /// * `url` - The address of your Qdrant instance (e.g., "http://localhost:6334").
    ///
    /// # Returns
    ///
    /// A Result containing a connected `VectorDB` or a `QdrantError`.
    pub fn new(url: &str) -> Result<Self, QdrantError> {
        let client = Qdrant::from_url(url).build()?;
        Ok(Self { client })
    }

    /// Ensures that a collection exists in Qdrant.
    ///
    /// This method first checks if the collection exists. If it exists and its status is
    /// either 0 (green) or 1 (yellow), it returns successfully. If the collection does not
    /// exist (or returns an error indicating "not found"), then it creates a new collection.
    /// After creation, it verifies that the collection’s status is acceptable (green or yellow).
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the collection.
    /// * `dims` - The dimensionality of the vectors.
    /// * `distance` - The distance metric to use (e.g., `Distance::Cosine`).
    ///
    /// # Returns
    ///
    /// A Result with unit type on success or a `QdrantError` on failure.
    pub async fn ensure_collection_exists(
        &self,
        name: &str,
        dims: usize,
        distance: Distance,
    ) -> Result<(), QdrantError> {
        // Check if the collection exists.
        match self.collection_info(name).await {
            Ok(response) => {
                if let Some(info) = response.result {
                    // Accept status 0 (green) or 1 (yellow) as healthy.
                    if info.status == 0 || info.status == 1 {
                        return Ok(());
                    } else {
                        return Err(QdrantError::ConversionError(format!(
                            "Collection {} exists but status is {}",
                            name, info.status
                        )));
                    }
                }
                // If result is None, fall through to creation.
            }
            Err(err) => {
                // If the error does not indicate "not found", return the error.
                if !err.to_string().contains("not found") {
                    return Err(err);
                }
            }
        }

        // Collection does not exist: create a new collection.
        self.client
            .create_collection(
                CreateCollectionBuilder::new(name)
                    .vectors_config(VectorParamsBuilder::new(dims.try_into().unwrap(), distance)),
            )
            .await?;

        // Verify that the collection now exists.
        let new_response = self.collection_info(name).await?;
        if let Some(new_info) = new_response.result {
            if new_info.status == 0 || new_info.status == 1 {
                Ok(())
            } else {
                Err(QdrantError::ConversionError(format!(
                    "Collection {} created but status is {}",
                    name, new_info.status
                )))
            }
        } else {
            Err(QdrantError::ConversionError(format!(
                "Collection {} info missing",
                name
            )))
        }
    }

    /// Retrieves collection information for the specified collection.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the collection.
    ///
    /// # Returns
    ///
    /// A Result containing a `GetCollectionInfoResponse` or a `QdrantError`.
    pub async fn collection_info(
        &self,
        name: &str,
    ) -> Result<GetCollectionInfoResponse, QdrantError> {
        self.client.collection_info(name).await
    }
}
