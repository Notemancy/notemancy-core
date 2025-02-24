// tests/integration_test.rs
mod common; // Imports tests/common/mod.rs

use common::setup_test_env;
use notemancy_core::config::load_config;
use notemancy_core::db::Database;
use notemancy_core::scan::Scanner;
use std::error::Error;

#[test]
fn test_scanner_with_ready_test_env() -> Result<(), Box<dyn Error>> {
    // Setup the test environment.
    setup_test_env(1000)?;

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
        1001,
        "Expected 100 markdown files scanned from the default vault."
    );

    // Run the image scan.
    scanner.scan_images()?;

    // Verify using the database queries:
    let db = Database::new()?;
    let pages = db.query_by_fields(&["path"])?;
    assert_eq!(
        pages.len(),
        1001,
        "Expected 101 pages (markdown files) in the pagetable."
    );

    let conn = db.connect()?;
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM attachments")?;
    let attachments_count: i64 = stmt.query_row([], |row| row.get(0))?;
    assert_eq!(
        attachments_count, 10,
        "Expected 10 attachments (images) in the attachments table."
    );

    Ok(())
}
