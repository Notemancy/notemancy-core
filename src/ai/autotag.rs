use crate::ai::nlputils::extract_candidate_phrases;
use crate::ai::sentence_transformer::generate_embedding;
use std::error::Error;

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Generate tags for an input text note.
///
/// The process is as follows:
/// 1. Generate an embedding for the overall text.
/// 2. Extract candidate phrases (unigrams and bigrams) using the nlputils module.
/// 3. For each candidate phrase, generate its embedding.
/// 4. Compute cosine similarity between the overall embedding and each candidate embedding.
/// 5. Return the top 3 candidate phrases with the highest similarity as tags.
pub fn generate_tags(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    // 1. Generate the overall embedding for the entire text.
    let overall_embeddings = generate_embedding(text)?;
    // Assume the first (or only) embedding represents the note.
    let overall_embedding = overall_embeddings
        .get(0)
        .ok_or("Failed to generate overall embedding")?;

    // 2. Extract candidate phrases from the text.
    let candidate_phrases = extract_candidate_phrases(text)?;

    // 3. For each candidate, generate its embedding and compute similarity.
    let mut candidate_scores = Vec::new();
    for candidate in candidate_phrases {
        let candidate_embeddings = generate_embedding(&candidate)?;
        let candidate_embedding = candidate_embeddings
            .get(0)
            .ok_or("Failed to generate candidate embedding")?;
        let similarity = cosine_similarity(overall_embedding, candidate_embedding);
        candidate_scores.push((candidate, similarity));
    }

    // 4. Sort candidates by similarity (highest first).
    candidate_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // 5. Select the top 3 candidate phrases as final tags.
    let final_tags = candidate_scores
        .into_iter()
        .take(3)
        .map(|(phrase, _sim)| phrase)
        .collect();

    Ok(final_tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_generate_tags_success() {
        // Determine the project root using the CARGO_MANIFEST_DIR environment variable.
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Set the configuration directory to be the "temp" folder in the project root.
        let config_dir = project_root.join("temp");

        // Set the NOTEMANCY_CONFIG_DIR environment variable so that our configuration is loaded from temp.
        env::set_var("NOTEMANCY_CONFIG_DIR", config_dir.to_str().unwrap());

        // Ensure the config directory exists.
        fs::create_dir_all(&config_dir).expect("Failed to create config directory");

        // Create a valid configuration file ("ncy.yaml") with minimal AI configuration.
        let config_file = config_dir.join("ncy.yaml");
        let config_content = r#"
ai:
  semantic_thresh: 0.5
"#;
        fs::write(&config_file, config_content).expect("Failed to write the configuration file");

        // Ensure the model directory exists.
        // This test assumes that the model in temp/paraphrase-albert-small-v2 is already set up correctly.
        // let model_dir = config_dir.join("paraphrase-albert-small-v2");
        let model_dir = config_dir.join("all-MiniLM-L6-v2");
        fs::create_dir_all(&model_dir).expect("Failed to create model directory");

        // Now call generate_tags with a sample note.
        let text = "Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.";
        let tags_result = generate_tags(text);
        assert!(
            tags_result.is_ok(),
            "generate_tags should succeed with valid config and model"
        );
        let tags = tags_result.unwrap();

        // Check that we got at least one tag, and no more than three.
        assert!(
            !tags.is_empty(),
            "Expected at least one tag to be generated"
        );
        assert!(
            tags.len() <= 3,
            "Expected no more than 3 tags to be generated"
        );

        println!("Generated tags: {:?}", tags);
    }
}
