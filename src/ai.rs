use crate::confapi::{get_config, get_config_dir};
use rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsBuilder;
use std::error::Error;
use std::path::PathBuf;
use tch;

pub fn generate_embedding(input_text: &str) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    // Load configuration from ncy.yaml in the config directory.
    let _config = get_config()?;
    let config_dir = get_config_dir();

    // Determine the model directory.
    // Since the new AIConfig does not include a model name, we default to "paraphrase-albert-small-v2".
    let model_dir: PathBuf = config_dir.join("paraphrase-albert-small-v2");

    // Build the model from the computed directory.
    let model =
        SentenceEmbeddingsBuilder::local(model_dir.to_str().ok_or("Invalid model directory path")?)
            .with_device(tch::Device::cuda_if_available())
            .create_model()?;

    // Generate embeddings for the provided input text.
    let sentences = [input_text];
    let embeddings = model.encode(&sentences)?;

    Ok(embeddings)
}
