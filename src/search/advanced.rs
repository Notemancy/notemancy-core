use super::SearchEngine;
use std::error::Error;
use std::fs;

/// Advanced snippet extraction that tries to find the most relevant
/// part of the document containing search terms
pub fn extract_relevant_snippet(
    content: &str,
    query_terms: &[&str],
    max_length: usize,
) -> Option<String> {
    if content.is_empty() || query_terms.is_empty() {
        return None;
    }

    // Split content into paragraphs (more meaningful units than lines)
    let paragraphs: Vec<&str> = content
        .split("\n\n")
        .filter(|p| !p.trim().is_empty() && !p.trim().starts_with('#'))
        .collect();

    if paragraphs.is_empty() {
        return None;
    }

    // Score each paragraph based on the presence of query terms
    let mut paragraph_scores: Vec<(usize, &str)> = paragraphs
        .iter()
        .map(|&p| {
            let p_lower = p.to_lowercase();
            let score = query_terms
                .iter()
                .filter(|&&term| p_lower.contains(&term.to_lowercase()))
                .count();
            (score, p)
        })
        .collect();

    // Sort by score, highest first
    paragraph_scores.sort_by(|a, b| b.0.cmp(&a.0));

    // Get the highest scoring paragraph
    let best_paragraph = paragraph_scores[0].1;

    // If no terms match, return the first paragraph
    if paragraph_scores[0].0 == 0 {
        let first_para = paragraphs[0];
        return Some(if first_para.len() > max_length {
            format!("{}...", &first_para[..max_length - 3])
        } else {
            first_para.to_string()
        });
    }

    // Try to locate a window within the paragraph containing the most query terms
    let words: Vec<&str> = best_paragraph.split_whitespace().collect();
    if words.len() <= 10 {
        // Paragraph is short enough to use as-is
        return Some(best_paragraph.to_string());
    }

    // For longer paragraphs, find the best window of ~10 words
    let window_size = 10.min(words.len());
    let mut best_window_score = 0;
    let mut best_window_start = 0;

    for start in 0..=words.len() - window_size {
        let window = words[start..start + window_size].join(" ").to_lowercase();
        let score = query_terms
            .iter()
            .filter(|&&term| window.contains(&term.to_lowercase()))
            .count();

        if score > best_window_score {
            best_window_score = score;
            best_window_start = start;
        }
    }

    // Extract the best window and a bit of context
    let start = best_window_start.saturating_sub(2);
    let end = (best_window_start + window_size + 2).min(words.len());
    let snippet = words[start..end].join(" ");

    // Add ellipsis if needed
    let mut result = String::new();
    if start > 0 {
        result.push_str("... ");
    }
    result.push_str(&snippet);
    if end < words.len() {
        result.push_str(" ...");
    }

    // Ensure the snippet doesn't exceed max_length
    if result.len() > max_length {
        Some(format!("{}...", &result[..max_length - 3]))
    } else {
        Some(result)
    }
}

/// Enhanced search engine with more advanced configurations
pub fn configure_enhanced_search(engine: &mut SearchEngine) -> Result<(), Box<dyn Error>> {
    let _ = engine;
    // This function would modify the search engine's index writer to use
    // more advanced configurations. This is a placeholder for such functionality.

    // Example: configure a custom tokenizer with stemming for better results
    // Note: This would require adjustments to the SearchEngine struct to expose
    // the tokenizer configuration methods

    Ok(())
}

/// Get statistics about the search index
pub fn get_index_stats(engine: &SearchEngine) -> Result<IndexStats, Box<dyn Error>> {
    let index_path = engine.get_index_path();
    let mut stats = IndexStats {
        num_documents: 0,
        index_size_bytes: 0,
        fields: vec![],
    };

    // Calculate the size of the index directory
    let mut size: u64 = 0;
    for entry in fs::read_dir(index_path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            size += metadata.len();
        }
    }
    stats.index_size_bytes = size;

    // Get number of documents and other stats from the reader
    // This requires more exposure from the SearchEngine struct to implement fully

    Ok(stats)
}

/// Statistics about the search index
pub struct IndexStats {
    pub num_documents: usize,
    pub index_size_bytes: u64,
    pub fields: Vec<String>,
}

/// Search relevance tuning parameters
pub struct RelevanceTuning {
    pub title_boost: f32,
    pub recent_boost: bool,
    pub fuzzy_search: bool,
    pub fuzzy_distance: u8,
}

impl Default for RelevanceTuning {
    fn default() -> Self {
        RelevanceTuning {
            title_boost: 2.0,
            recent_boost: true,
            fuzzy_search: true,
            fuzzy_distance: 1,
        }
    }
}
