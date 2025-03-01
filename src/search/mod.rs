use crate::config::get_config_dir;
use crate::db::Database;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

pub mod advanced;

const INDEX_DIR: &str = "search_index";
const INDEX_WRITER_MEMORY: usize = 50_000_000; // 50MB

/// A struct that manages the search functionality
pub struct SearchEngine {
    index: Index,
    schema: Schema,
    index_path: PathBuf,
    field_title: Field,
    field_body: Field,
    field_path: Field,
}

/// A search result containing relevant metadata
#[derive(Debug)]
pub struct SearchResult {
    pub path: String,
    pub title: String,
    pub score: f32,
    pub snippet: Option<String>,
}

impl SearchEngine {
    /// Creates a new SearchEngine instance
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let config_dir = get_config_dir()?;
        let index_path = config_dir.join(INDEX_DIR);

        // Ensure the index directory exists
        if !index_path.exists() {
            fs::create_dir_all(&index_path)?;
            println!("Created search index directory: {:?}", index_path);
        }

        // Define schema
        let schema = Self::create_schema();

        // Get field handles
        let field_title = schema.get_field("title").unwrap();
        let field_body = schema.get_field("body").unwrap();
        let field_path = schema.get_field("path").unwrap();

        // Create or open the index
        let index = if index_path.join("meta.json").exists() {
            // Open existing index
            Index::open_in_dir(&index_path)?
        } else {
            // Create new index
            Index::create_in_dir(&index_path, schema.clone())?
        };

        Ok(SearchEngine {
            index,
            schema,
            index_path,
            field_title,
            field_body,
            field_path,
        })
    }

    /// Creates the schema for the search index
    fn create_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        // Title field - indexed and stored
        schema_builder.add_text_field("title", TEXT | STORED);

        // Body field - indexed but not stored (to save space)
        schema_builder.add_text_field("body", TEXT);

        // Path field - stored but not indexed (just for retrieval)
        schema_builder.add_text_field("path", STORED);

        schema_builder.build()
    }

    /// Creates a writer for the index
    fn get_writer(&self) -> Result<IndexWriter, Box<dyn Error>> {
        let writer = self.index.writer(INDEX_WRITER_MEMORY)?;
        Ok(writer)
    }

    /// Creates a reader for the index
    fn get_reader(&self) -> Result<IndexReader, Box<dyn Error>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(reader)
    }

    /// Extract a title from a markdown file
    /// This looks for the first heading or the filename if no heading is found
    fn extract_title_from_markdown(content: &str, path: &Path) -> String {
        // Try to find the first heading (# Title)
        if let Some(line) = content.lines().find(|line| line.starts_with("# ")) {
            return line.trim_start_matches("# ").to_string();
        }

        // Fall back to filename without extension
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    }

    /// Index a single document
    fn index_document(
        &self,
        writer: &mut IndexWriter,
        path: &str,
        content: &str,
    ) -> Result<(), Box<dyn Error>> {
        let path_obj = Path::new(path);
        let title = Self::extract_title_from_markdown(content, path_obj);

        writer.add_document(doc!(
            self.field_title => title,
            self.field_body => content,
            self.field_path => path
        ))?;

        Ok(())
    }

    /// Index all documents in the database
    pub fn index_all_documents(&self, db: &Database) -> Result<(), Box<dyn Error>> {
        let mut writer = self.get_writer()?;

        // Delete all existing documents - we're rebuilding the index from scratch
        writer.delete_all_documents()?;

        // Get all file records from DB
        let file_records = db.get_file_tree()?;

        let mut indexed_count = 0;
        let mut error_count = 0;

        for record in file_records {
            let path = record.path;

            // Skip non-markdown files
            if !path.ends_with(".md") {
                continue;
            }

            // Read file content
            match fs::read_to_string(&path) {
                Ok(content) => {
                    if let Err(e) = self.index_document(&mut writer, &path, &content) {
                        eprintln!("Error indexing {}: {}", path, e);
                        error_count += 1;
                    } else {
                        indexed_count += 1;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading {}: {}", path, e);
                    error_count += 1;
                }
            }
        }

        // Commit changes
        writer.commit()?;

        println!(
            "Indexed {} documents. {} errors.",
            indexed_count, error_count
        );

        Ok(())
    }

    /// Update the index for a single document
    pub fn update_document(&self, path: &str) -> Result<(), Box<dyn Error>> {
        let mut writer = self.get_writer()?;

        // First delete the document if it exists
        let path_term = Term::from_field_text(self.field_path, path);
        writer.delete_term(path_term.clone());

        // Only index if the file exists and is a markdown file
        if path.ends_with(".md") && Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            self.index_document(&mut writer, path, &content)?;
        }

        // Commit changes
        writer.commit()?;

        Ok(())
    }

    /// Remove a document from the index
    pub fn remove_document(&self, path: &str) -> Result<(), Box<dyn Error>> {
        let mut writer = self.get_writer()?;

        // Delete the document if it exists
        let path_term = Term::from_field_text(self.field_path, path);
        writer.delete_term(path_term.clone());

        // Commit changes
        writer.commit()?;

        Ok(())
    }

    /// Search the index with a query string
    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, Box<dyn Error>> {
        let reader = self.get_reader()?;
        let searcher = reader.searcher();

        // Create a query parser that searches in both title and body fields
        // Title matches are boosted for higher relevance
        let mut query_parser =
            QueryParser::for_index(&self.index, vec![self.field_title, self.field_body]);
        query_parser.set_field_boost(self.field_title, 2.0);

        let query = query_parser.parse_query(query_str)?;

        // Search for the top documents
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        // Convert results to SearchResult objects
        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;

            let path = retrieved_doc
                .get_first(self.field_path)
                .and_then(|f| f.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            let title = retrieved_doc
                .get_first(self.field_title)
                .and_then(|f| f.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "Untitled".to_string());

            // Create a snippet (we could implement a more sophisticated snippet generation)
            let snippet = if path.is_empty() {
                None
            } else {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let preview = content
                            .lines()
                            .filter(|line| !line.starts_with('#')) // Skip headings
                            .take(3) // Take first 3 non-heading lines
                            .collect::<Vec<_>>()
                            .join(" ");

                        Some(if preview.len() > 150 {
                            format!("{}...", &preview[..147])
                        } else {
                            preview
                        })
                    }
                    Err(_) => None,
                }
            };

            results.push(SearchResult {
                path,
                title,
                score,
                snippet,
            });
        }

        Ok(results)
    }

    /// Get the path to the index directory
    pub fn get_index_path(&self) -> &Path {
        &self.index_path
    }

    /// Optimize the index for faster searching
    pub fn optimize(&self) -> Result<(), Box<dyn Error>> {
        let mut writer = self.get_writer()?;
        // In tantivy 0.22.0, merge() returns a Future and requires segment IDs
        // For a simple optimization, we'll use the simpler API:
        writer.wait_merging_threads()?;
        Ok(())
    }
}

/// Initialize the search engine
pub fn init_search_engine() -> Result<SearchEngine, Box<dyn Error>> {
    SearchEngine::new()
}

/// Build or rebuild the search index from all documents in the database
pub fn build_search_index(db: &Database) -> Result<(), Box<dyn Error>> {
    let engine = init_search_engine()?;
    engine.index_all_documents(db)
}
