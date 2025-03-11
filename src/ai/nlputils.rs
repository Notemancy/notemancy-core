use crate::confapi::get_config_dir;
use nlprule::Tokenizer;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;

// Add the stemmer crate.
use rust_stemmers::{Algorithm, Stemmer};

/// Extract candidate phrases from the text.
///
/// Unigrams are added if their POS tag is "JJ" (adjective) or starts with "NN" (noun).
/// Bigrams are added only if both tokens are candidate tokens and if they are not both nouns.
/// The candidates are normalized (trimmed and lowercased) and deduplicated. Additionally,
/// for single-word candidates we apply stemming to remove variations (e.g. "certificates" and "certificate").
pub fn extract_candidate_phrases(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    // Build the path to "en_tokenizer.bin" in the config directory.
    let mut tokenizer_path: PathBuf = get_config_dir();
    tokenizer_path.push("en_tokenizer.bin");
    let tokenizer_path_str = tokenizer_path.to_str().ok_or("Invalid tokenizer path")?;

    // Initialize the tokenizer from the binary file.
    let tokenizer = Tokenizer::new(tokenizer_path_str)?;

    // Use a HashSet to deduplicate candidates.
    let mut candidates_set: HashSet<String> = HashSet::new();

    // Process each sentence.
    for sentence in tokenizer.pipe(text) {
        let tokens = sentence.tokens();

        // Helper: check if token is a candidate.
        let is_candidate = |token: &nlprule::types::Token| -> bool {
            let pos: &str = token.word().tags()[0].pos().as_str();
            pos == "JJ" || pos.starts_with("NN")
        };

        // Extract unigrams.
        for token in tokens.iter() {
            if is_candidate(token) {
                let word = token.word().text().as_str().trim().to_lowercase();
                if !word.is_empty() {
                    candidates_set.insert(word);
                }
            }
        }

        // Extract bigrams.
        // Only include bigrams if both tokens are candidates and
        // if they are not both nouns.
        for window in tokens.windows(2) {
            if is_candidate(&window[0]) && is_candidate(&window[1]) {
                let pos1 = window[0].word().tags()[0].pos().as_str();
                let pos2 = window[1].word().tags()[0].pos().as_str();
                // If both tokens are nouns, skip the bigram.
                if pos1.starts_with("NN") && pos2.starts_with("NN") {
                    continue;
                }
                let word1 = window[0].word().text().as_str().trim().to_lowercase();
                let word2 = window[1].word().text().as_str().trim().to_lowercase();
                if !word1.is_empty() && !word2.is_empty() {
                    let phrase = format!("{} {}", word1, word2);
                    candidates_set.insert(phrase);
                }
            }
        }
    }

    // Use the rust_stemmers crate to create an English stemmer.
    let stemmer = Stemmer::create(Algorithm::English);

    // For unigrams, deduplicate by stem.
    let mut stem_map: HashMap<String, String> = HashMap::new();
    let mut multi_word_candidates: Vec<String> = Vec::new();

    for candidate in candidates_set.into_iter() {
        if candidate.contains(' ') {
            // For multi-word phrases, we keep them as is.
            multi_word_candidates.push(candidate);
        } else {
            // For single words, compute the stem.
            let stem = stemmer.stem(&candidate).into_owned();
            // If there is already a candidate for this stem, choose the shorter one.
            stem_map
                .entry(stem)
                .and_modify(|existing| {
                    if candidate.len() < existing.len() {
                        *existing = candidate.clone();
                    }
                })
                .or_insert(candidate);
        }
    }

    // Combine deduplicated unigrams and multi-word candidates.
    let mut final_candidates: Vec<String> = stem_map.into_values().collect();
    final_candidates.extend(multi_word_candidates);
    final_candidates.sort();

    Ok(final_candidates)
}
