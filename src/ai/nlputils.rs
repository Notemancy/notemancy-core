use crate::confapi::get_config_dir;
use nlprule::Tokenizer;
use std::error::Error;
use std::path::PathBuf;

pub fn extract_candidate_phrases(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    // Build the path to "en_tokenizer.bin" in the config directory.
    let mut tokenizer_path: PathBuf = get_config_dir();
    tokenizer_path.push("en_tokenizer.bin");
    let tokenizer_path_str = tokenizer_path.to_str().ok_or("Invalid tokenizer path")?;

    // Initialize the tokenizer from the binary file.
    let tokenizer = Tokenizer::new(tokenizer_path_str)?;
    let mut candidates = Vec::new();

    // Process each sentence in the text.
    for sentence in tokenizer.pipe(text) {
        let tokens = sentence.tokens();

        // Helper closure: consider a token a candidate if its first tag is "JJ" or starts with "NN".
        let is_candidate = |token: &nlprule::types::Token| -> bool {
            // Use `pos().as_str()` to obtain a &str for the POS tag.
            let pos: &str = token.word().tags()[0].pos().as_str();
            pos == "JJ" || pos.starts_with("NN")
        };

        // Extract unigrams.
        for token in tokens.iter() {
            if is_candidate(token) {
                // Use as_str() to obtain a &str, then convert to String.
                candidates.push(token.word().text().as_str().to_string());
            }
        }

        // Extract bigrams.
        for window in tokens.windows(2) {
            if is_candidate(&window[0]) && is_candidate(&window[1]) {
                let phrase = format!(
                    "{} {}",
                    window[0].word().text().as_str(),
                    window[1].word().text().as_str()
                );
                candidates.push(phrase);
            }
        }
    }

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use crate::ai::nlputils::extract_candidate_phrases;
    use crate::confapi::{get_config_dir, get_config_file_path};
    use std::env;
    use std::path::PathBuf;

    /// Test that the NOTEMANCY_CONFIG_DIR environment variable is honored.
    #[test]
    fn test_get_config_dir_override() {
        // Set the config directory to the "temp" folder inside the project root.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let expected = PathBuf::from(manifest_dir).join("temp");
        env::set_var("NOTEMANCY_CONFIG_DIR", expected.to_str().unwrap());

        // Now get_config_dir() should return the expected path.
        let config_dir = get_config_dir();
        assert_eq!(
            config_dir, expected,
            "Config dir should be overridden by NOTEMANCY_CONFIG_DIR"
        );
    }

    /// Test that get_config_file_path returns the correct file path under the override.
    #[test]
    fn test_get_config_file_path_override() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let expected = PathBuf::from(manifest_dir).join("temp").join("ncy.yaml");
        env::set_var(
            "NOTEMANCY_CONFIG_DIR",
            expected.parent().unwrap().to_str().unwrap(),
        );

        let config_file = get_config_file_path();
        assert_eq!(
            config_file, expected,
            "Config file path should be correct under override"
        );
    }

    /// Test candidate phrase extraction using the tokenizer binary from the overridden config directory.
    #[test]
    fn test_extract_candidate_phrases() -> Result<(), Box<dyn std::error::Error>> {
        // Set the config directory to the "temp" folder inside the project root.
        // (Ensure that `temp/en_tokenizer.bin` exists and is a valid tokenizer binary.)
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let config_override = PathBuf::from(manifest_dir).join("temp");
        env::set_var("NOTEMANCY_CONFIG_DIR", config_override.to_str().unwrap());

        // Optionally, you can print the resolved path for debugging:
        println!("Using config dir: {:?}", crate::confapi::get_config_dir());
        println!(
            "Using config file: {:?}",
            crate::confapi::get_config_file_path()
        );

        // Now call extract_candidate_phrases on a sample sentence.
        let text = "A brief example is shown.";
        let candidates = extract_candidate_phrases(text)?;

        // For example, if the tokenizer returns tags similar to your sample,
        // we might expect the adjective "brief", the noun "example", and the bigram "brief example".
        // We'll simply check that some expected candidate appears.
        assert!(
            candidates.contains(&"brief".to_string()),
            "Candidates: {:?}",
            candidates
        );
        assert!(
            candidates.contains(&"example".to_string()),
            "Candidates: {:?}",
            candidates
        );
        assert!(
            candidates.contains(&"brief example".to_string()),
            "Candidates: {:?}",
            candidates
        );

        Ok(())
    }
}
