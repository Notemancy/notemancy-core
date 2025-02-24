// src/embeddings.rs

use anyhow::Result;
use rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsBuilder;
use tch::Device;

/// Runs sentence embeddings on a hard-coded example.
///
/// This function sets up the model (from the local resource folder),
/// encodes two sentences, and prints the resulting embeddings.
pub fn run_sentence_embeddings() -> Result<()> {
    // Set-up sentence embeddings model from the resources folder.
    let model = SentenceEmbeddingsBuilder::local("resources/all-MiniLM-L12-v2")
        .with_device(Device::cuda_if_available())
        .create_model()?;

    // Define the input sentences.
    let sentences = ["this is an example sentence", "each sentence is converted"];

    // Generate embeddings.
    let embeddings = model.encode(&sentences)?;
    println!("{:?}", embeddings);

    Ok(())
}
