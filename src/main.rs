use anyhow::Result;
use rust_bert::pipelines::sentence_embeddings::SentenceEmbeddingsBuilder;
use tch::Device;

fn main() -> Result<()> {
    // Using an absolute path from the Cargo manifest directory:
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let model_path = format!("{}/resources/all-MiniLM-L12-v2", manifest_dir);
    println!("{:?}", model_path);

    // Create the model using the local resource folder
    let model = SentenceEmbeddingsBuilder::local(&model_path)
        .with_device(Device::cuda_if_available())
        .create_model()?;

    // Define input sentences
    let sentences = ["this is an example sentence", "each sentence is converted"];

    // Generate embeddings
    let embeddings = model.encode(&sentences)?;
    println!("{:?}", embeddings);

    Ok(())
}
