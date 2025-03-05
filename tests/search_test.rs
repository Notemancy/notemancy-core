mod common; // imports tests/common/mod.rs
use common::setup_test_env;
use notemancy_core::config::load_config;
use notemancy_core::db::Database;
use notemancy_core::scan::Scanner;
use notemancy_core::search::init_search_engine;
use std::error::Error;
use std::path::PathBuf;

#[test]
fn integration_search() -> Result<(), Box<dyn Error>> {
    // 1. Set up the test environment with sample files
    setup_test_env(1000)?;

    // 2. Load the configuration
    let config = load_config()?;
    println!("Test config loaded: {:?}", config);

    // 3. Create a scanner from configuration
    let scanner = Scanner::from_config()?;

    // 4. Run the markdown scan and expect 1001 files
    let (md_files, summary) = scanner.scan_markdown_files()?;
    println!("Scan Summary:\n{}", summary);
    assert_eq!(
        md_files.len(),
        1001,
        "Expected 1001 markdown files scanned from the default vault."
    );

    // 5. Retrieve file paths from the database
    let db = Database::new()?;
    let pages = db.query_by_fields(&["path"])?;
    let file_paths: Vec<PathBuf> = pages
        .iter()
        .filter_map(|m| m.get("path").map(PathBuf::from))
        .collect();

    assert!(
        !file_paths.is_empty(),
        "No file paths found in the database."
    );

    // 6. Initialize the search engine
    let search_engine = init_search_engine()?;

    // 7. Index all documents from the database
    println!("Indexing documents for search...");
    search_engine.index_all_documents(&db)?;
    println!("Indexing completed.");

    // 8. Define and run queries; assert each returns at least one match
    let queries = vec!["wiki", "links", "the", "what is up"];

    for query in queries {
        let results = search_engine.search(query, 10)?;
        println!("Query: '{}', results count: {}", query, results.len());

        // Print the top 3 results for inspection
        for (i, result) in results.iter().take(3).enumerate() {
            println!(
                "  Result {}: '{}' (score: {:.2}) - {}",
                i + 1,
                result.title,
                result.score,
                result.path
            );
            if let Some(snippet) = &result.snippet {
                println!("    Snippet: \"{}\"", snippet);
            }
        }

        assert!(
            !results.is_empty(),
            "Expected at least 1 match for query: '{}', but got none.",
            query
        );
    }

    // 9. Test updating a document
    if let Some(first_path) = file_paths.first() {
        println!("Testing document update functionality...");
        let path_str = first_path.to_string_lossy().to_string();
        search_engine.update_document(&path_str)?;
        println!("Document update successful.");
    }

    // 10. Test optimization
    println!("Testing index optimization...");
    search_engine.optimize()?;
    println!("Index optimization successful.");

    Ok(())
}
