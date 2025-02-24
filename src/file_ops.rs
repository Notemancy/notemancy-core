// src/file/mod.rs

use crate::db::Database;
use rusqlite::params;
use std::error::Error;
use std::fs;
use std::path::Path;

/// Updates the content of a markdown file on disk.
///
/// One (and only one) of `path` or `virtual_path` must be provided.
/// The function will look up the file record in the database and, if found,
/// will write the provided `content` to the file at the stored path.
///
/// # Arguments
///
/// * `content` - The new markdown content to write into the file.
/// * `path` - The filesystem path of the file (optional).
/// * `virtual_path` - The virtual path identifier of the file (optional).
///
/// # Errors
///
/// Returns an error if:
/// - Neither `path` nor `virtual_path` is provided.
/// - The file record is not found in the database.
/// - The file does not exist on disk.
/// - Writing to the file fails.
pub fn update_markdown_file(
    content: &str,
    path: Option<&str>,
    virtual_path: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    // Validate that at least one identifier was provided.
    if path.is_none() && virtual_path.is_none() {
        return Err("Either `path` or `virtual_path` must be provided.".into());
    }

    // Create a new database instance.
    let db = Database::new()?;
    let conn = db.connect()?;

    // Decide which identifier to use.
    // Here, if both are provided we choose `path` over `virtual_path`.
    let (sql, identifier) = if let Some(p) = path {
        ("SELECT path FROM pagetable WHERE path = ?1", p)
    } else if let Some(vp) = virtual_path {
        ("SELECT path FROM pagetable WHERE virtualPath = ?1", vp)
    } else {
        unreachable!(); // Already validated above.
    };

    // Query the database for the file record.
    let file_path: String = conn
        .query_row(sql, params![identifier], |row| row.get(0))
        .map_err(|_| "No file record found with the provided identifier.")?;

    // Validate that the file exists.
    if !Path::new(&file_path).exists() {
        return Err(format!("File does not exist at path: {}", file_path).into());
    }

    // Write the provided content to the file.
    fs::write(&file_path, content)?;
    println!("Updated file at path: {}", file_path);
    Ok(())
}
