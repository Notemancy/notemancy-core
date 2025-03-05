mod common; // Imports tests/common/mod.rs

use common::setup_test_env;
use notemancy_core::config::load_config;
use notemancy_core::db::Database;
use notemancy_core::fetch::Fetch;
use notemancy_core::scan::Scanner;
use rand::Rng;
use std::error::Error;
use std::fs;

#[test]
fn test_fetch_get_page_content() -> Result<(), Box<dyn Error>> {
    setup_test_env(300)?;

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
        301,
        "Expected 100 markdown files scanned from the default vault."
    );
    // Query the pagetable for virtualPath, local path, and metadata.
    let db = Database::new()?;
    let pages = db.query_by_fields(&["virtualPath", "path", "metadata"])?;
    assert!(
        !pages.is_empty(),
        "No pages found in the pagetable for testing get_page_content."
    );

    // Pick one file at random.
    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..pages.len());
    let page = &pages[index];
    let virtual_path = page.get("virtualPath").expect("virtualPath field missing");
    let local_path = page.get("path").expect("path field missing");
    let expected_metadata = page.get("metadata").expect("metadata field missing");

    // Use our Fetch API to get the file's content and metadata by its virtual path.
    let fetch = Fetch::new()?;
    let fetched_page = fetch.get_page_content(virtual_path)?;

    // Manually read the file's content using its local path.
    let manual_content = fs::read_to_string(local_path)?;

    // Compare the file content.
    assert_eq!(
        fetched_page.content, manual_content,
        "The content fetched via Fetch does not match the file's actual content."
    );

    // Check that the metadata is as expected.
    assert_eq!(
        fetched_page.metadata, *expected_metadata,
        "The metadata fetched via Fetch does not match the expected metadata from the database."
    );

    // Additionally, fetch metadata for "home.md" and print it to the console.
    let home_page = fetch.get_page_content("home.md")?;
    println!("Metadata for home.md: {}", home_page.metadata);

    Ok(())
}
