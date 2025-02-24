// tests/integration_search.rs
mod common; // imports tests/common/mod.rs

use common::setup_test_env;
use notemancy_core::config::load_config;
use notemancy_core::db::Database;
use notemancy_core::scan::Scanner;
use notemancy_core::search::{MeiliSearchServer, SearchInterface};
use std::error::Error;
use std::path::PathBuf;

#[tokio::test]
async fn integration_search() -> Result<(), Box<dyn Error>> {
    setup_test_env(100)?;

    // Load the configuration.
    let config = load_config()?;
    println!("Test config loaded: {:?}", config);

    // Create a scanner from configuration.
    let scanner = Scanner::from_config()?;

    // Run the markdown scan and expect 100 files.
    let (md_files, summary) = scanner.scan_markdown_files()?;
    println!("Scan Summary:\n{}", summary);
    assert_eq!(
        md_files.len(),
        101,
        "Expected 100 markdown files scanned from the default vault."
    );

    // 3. Retrieve file paths from the database.
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

    // 4. Start the MeiliSearch server (with dynamic port picking).
    let mut server = MeiliSearchServer::start()?;
    let base_url = format!("http://127.0.0.1:{}", server.port);

    // 5. Create the search interface using the dynamic base URL.
    let search_interface = SearchInterface::new_with_url(&base_url)?;

    // 6. Index the files from the database.
    search_interface.index_files(file_paths).await?;

    // 7. Define and run queries; assert each returns more than one match.
    let queries = vec!["wiki", "links", "the", "what is up"];
    for query in queries {
        let results = search_interface.search(query).await?;
        println!("Query: '{}', results count: {}", query, results.len());
        assert!(
            results.len() > 1,
            "Expected more than 1 match for query: '{}', but got {}.",
            query,
            results.len()
        );
    }

    // 8. Shutdown the MeiliSearch server.
    server.shutdown()?;

    Ok(())
}
