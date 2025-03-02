// src/model_setup.rs
use crate::config;
use anyhow::{anyhow, Result};
use rust_bert::pipelines::sentence_embeddings::{
    SentenceEmbeddingsBuilder, SentenceEmbeddingsModelType,
};
use std::fs;
use std::path::Path;

/// Ensures the embedding model is available for use
pub async fn ensure_model_available() -> Result<()> {
    let config_dir =
        config::get_config_dir().map_err(|e| anyhow!("Failed to get config directory: {}", e))?;

    // Model name - hardcoded since we only support one model
    let model_name = "all-MiniLM-L12-v2";
    let model_path = config_dir.join(model_name);

    // If the model directory doesn't exist, create it and download the model
    if !model_path.exists() {
        println!(
            "Embedding model not found. Downloading to {:?}...",
            model_path
        );

        // Create the directory
        fs::create_dir_all(&model_path)
            .map_err(|e| anyhow!("Failed to create model directory: {}", e))?;

        // Download the model into this directory
        download_model(&model_path).map_err(|e| anyhow!("Failed to download model: {}", e))?;

        println!("Model downloaded successfully.");
    }

    Ok(())
}

/// Downloads the all-MiniLM-L12-v2 model to the specified path
fn download_model(model_path: &Path) -> Result<()> {
    // This is a bit of a hack - we're using rust-bert's remote model functionality
    // to get the model, then we'll just keep it in our target location

    // Download the model - this will cache it in the rust-bert default location
    // Here we use the correct enum value instead of a string
    let _temp_model =
        SentenceEmbeddingsBuilder::remote(SentenceEmbeddingsModelType::AllMiniLmL12V2)
            .create_model()
            .map_err(|e| anyhow!("Failed to download model: {}", e))?;

    // Now find where rust-bert cached it - we know it's in the default cache location

    // On Unix-like systems (Linux, macOS):
    // $HOME/.cache/huggingface/hub/models--sentence-transformers--all-MiniLM-L12-v2

    // On Windows:
    // C:\Users\username\AppData\Local\huggingface\hub\models--sentence-transformers--all-MiniLM-L12-v2

    let cache_dir = if cfg!(windows) {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .map_err(|e| anyhow!("Failed to get LOCALAPPDATA environment variable: {}", e))?;
        Path::new(&local_app_data).join("huggingface").join("hub")
    } else {
        let home_dir = std::env::var("HOME")
            .map_err(|e| anyhow!("Failed to get HOME environment variable: {}", e))?;
        Path::new(&home_dir)
            .join(".cache")
            .join("huggingface")
            .join("hub")
    };

    let model_cache_path = cache_dir
        .join("models--sentence-transformers--all-MiniLM-L12-v2")
        .join("snapshots");

    // Find the snapshot directory (should have a hash as its name)
    if !model_cache_path.exists() {
        return Err(anyhow!(
            "Expected model cache not found at: {:?}",
            model_cache_path
        ));
    }

    // Get the first snapshot directory
    let snapshots = fs::read_dir(&model_cache_path).map_err(|e| {
        anyhow!(
            "Failed to read snapshots directory: {:?}, error: {}",
            model_cache_path,
            e
        )
    })?;

    let snapshot_dir = snapshots
        .filter_map(Result::ok)
        .next()
        .ok_or_else(|| anyhow!("No snapshot found in {:?}", model_cache_path))?
        .path();

    // Now copy all files from the snapshot to our model_path
    for entry in fs::read_dir(&snapshot_dir)
        .map_err(|e| {
            anyhow!(
                "Failed to read snapshot directory: {:?}, error: {}",
                snapshot_dir,
                e
            )
        })?
        .filter_map(Result::ok)
    {
        let file_path = entry.path();
        if file_path.is_file() {
            let file_name = file_path.file_name().unwrap();
            let dest_path = model_path.join(file_name);
            fs::copy(&file_path, &dest_path).map_err(|e| {
                anyhow!(
                    "Failed to copy file from {:?} to {:?}, error: {}",
                    file_path,
                    dest_path,
                    e
                )
            })?;
        }
    }

    println!("Model files copied to {:?}", model_path);
    Ok(())
}

/// A more manual approach to download the model
/// This can be used if the automatic download doesn't work
pub fn print_manual_download_instructions() {
    println!("To manually download the all-MiniLM-L12-v2 model:");
    println!("1. Go to https://huggingface.co/sentence-transformers/all-MiniLM-L12-v2/tree/main");
    println!("2. Download all files from the repository");

    if let Ok(model_path) = config::get_config_dir() {
        let model_dir = model_path.join("all-MiniLM-L12-v2");
        println!("3. Create the directory: {:?}", model_dir);
        println!("4. Place all downloaded files in this directory");
    } else {
        println!(
            "3. Place all files in the 'all-MiniLM-L12-v2' directory in your config directory"
        );
    }

    println!("5. Restart the application");
}
