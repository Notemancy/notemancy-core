use crate::config::{get_config_dir, load_config};
use rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsBuilder;
use std::error::Error;
use std::path::PathBuf;
use tch;

pub fn generate_embedding(input_text: &str) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    // Load configuration from config.yaml in the config directory.
    let config = load_config()?;
    let config_dir = get_config_dir()?;

    // Determine the model directory.
    // If the AI configuration contains a model_name, we assume the model is located in the config directory.
    // Otherwise, we fallback to a default subdirectory.
    let model_dir: PathBuf = if let Some(ai_config) = config.ai {
        if let Some(model_name) = ai_config.model_name {
            config_dir.join(model_name)
        } else {
            config_dir.join("paraphrase-albert-small-v2")
        }
    } else {
        config_dir.join("paraphrase-albert-small-v2")
    };

    // Build the model from the computed directory.
    let model =
        SentenceEmbeddingsBuilder::local(model_dir.to_str().ok_or("Invalid model directory path")?)
            .with_device(tch::Device::cuda_if_available())
            .create_model()?;

    // Generate embeddings for the provided input text.
    // Here we wrap the input_text in a slice so that it can be processed.
    let sentences = [input_text];
    let embeddings = model.encode(&sentences)?;

    Ok(embeddings)
}
