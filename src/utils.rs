use crate::dbapi::{self, delete_record, get_db_file_path, run_migrations, RecordIdentifier};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

/// Returns an array of strings containing the lpaths and/or vpaths from all records in the pagetable.
/// If both booleans are true, both fields will be included (in order per record).
pub fn get_all_paths(
    include_lpath: bool,
    include_vpath: bool,
) -> Result<Vec<String>, dbapi::DbError> {
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;
    let mut fields = Vec::new();
    if include_lpath {
        fields.push("lpath");
    }
    if include_vpath {
        fields.push("vpath");
    }
    if fields.is_empty() {
        return Ok(Vec::new());
    }
    let query = format!("SELECT {} FROM pagetable", fields.join(", "));
    let mut stmt = conn.prepare(&query)?;
    let mut rows = stmt.query([])?;
    let mut results = Vec::new();
    while let Some(row) = rows.next()? {
        if include_lpath && include_vpath {
            let l: String = row.get("lpath")?;
            let v: String = row.get("vpath")?;
            results.push(l);
            results.push(v);
        } else if include_lpath {
            let l: String = row.get("lpath")?;
            results.push(l);
        } else if include_vpath {
            let v: String = row.get("vpath")?;
            results.push(v);
        }
    }
    Ok(results)
}

/// Iterates through all lpaths in the database and deletes the record if the file does not exist on disk.
pub fn cleanup_stale_records() -> Result<(), dbapi::DbError> {
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;
    let mut stmt = conn.prepare("SELECT lpath FROM pagetable")?;
    let lpath_iter = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut stale_paths = Vec::new();
    for lpath_result in lpath_iter {
        let lpath: String = lpath_result?;
        if !Path::new(&lpath).exists() {
            stale_paths.push(lpath);
        }
    }
    for l in stale_paths {
        delete_record(RecordIdentifier::Lpath(l))?;
    }
    Ok(())
}

/// Given a vpath, returns the corresponding lpath from the database.
pub fn get_lpath(vpath: &str) -> Result<Option<String>, dbapi::DbError> {
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;
    let mut stmt = conn.prepare("SELECT lpath FROM pagetable WHERE vpath = ?")?;
    let result = stmt.query_row([vpath], |row| row.get(0)).optional()?;
    Ok(result)
}

/// Reads a file from disk.
/// You must supply at least one of `lpath` or `vpath`. If only `vpath` is provided, the function
/// will lookup the corresponding lpath from the database.
/// The `metadata` flag (default true) indicates whether to keep YAML frontmatter.
/// If false, the returned content is stripped of YAML frontmatter.
pub fn read_file(
    lpath: Option<&str>,
    vpath: Option<&str>,
    metadata: bool,
) -> Result<String, Box<dyn Error>> {
    let path_str = if let Some(l) = lpath {
        l.to_string()
    } else if let Some(v) = vpath {
        match get_lpath(v)? {
            Some(found) => found,
            None => return Err("No corresponding lpath found for provided vpath".into()),
        }
    } else {
        return Err("At least one of lpath or vpath must be provided".into());
    };

    let content = fs::read_to_string(&path_str)?;
    if metadata {
        Ok(content)
    } else {
        // If content starts with YAML frontmatter delimited by '---'
        if content.trim_start().starts_with("---") {
            // Split into at most three parts: before frontmatter (often empty), frontmatter, and content.
            let parts: Vec<&str> = content.splitn(3, "---").collect();
            if parts.len() == 3 {
                // Return the content after the frontmatter.
                return Ok(parts[2].trim_start().to_string());
            }
        }
        Ok(content)
    }
}

/// Extracts and returns the YAML frontmatter (if any) from the file at the given lpath.
pub fn get_metadata(lpath: &str) -> Result<Option<String>, Box<dyn Error>> {
    let content = fs::read_to_string(lpath)?;
    if content.trim_start().starts_with("---") {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let metadata = parts[1].trim().to_string();
            return Ok(Some(metadata));
        }
    }
    Ok(None)
}

pub fn get_records_by_column(
    columns: &[&str],
) -> Result<Vec<HashMap<String, Option<String>>>, Box<dyn Error>> {
    // List of allowed column names.
    let allowed = ["id", "lpath", "title", "timestamp", "vpath", "project"];
    // Validate that each requested column is allowed.
    for &col in columns {
        if !allowed.contains(&col) {
            return Err(format!("Invalid column: {}", col).into());
        }
    }

    // If no columns are provided, return an empty vector.
    if columns.is_empty() {
        return Ok(Vec::new());
    }

    // Ensure migrations have been run.
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;

    // Build the query using the specified columns.
    let query = format!("SELECT {} FROM pagetable", columns.join(", "));
    let mut stmt = conn.prepare(&query)?;
    let mut rows = stmt.query([])?;
    let mut records = Vec::new();

    while let Some(row) = rows.next()? {
        let mut record = HashMap::new();
        for &col in columns {
            if col == "id" {
                // 'id' is stored as an integer.
                let value: i64 = row.get(col)?;
                record.insert(col.to_string(), Some(value.to_string()));
            } else {
                // Other fields are stored as text; they might be NULL so we use Option<String>.
                let value: Option<String> = row.get(col)?;
                record.insert(col.to_string(), value);
            }
        }
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_file_without_metadata() {
        // Create a temporary file with YAML frontmatter.
        let mut file = NamedTempFile::new().unwrap();
        let content = "\
---
title: Test Document
date: 2025-03-08
---
This is the body of the document.
";
        write!(file, "{}", content).unwrap();
        let file_path = file.path().to_str().unwrap();

        // When metadata is false, the YAML frontmatter should be stripped.
        let body = read_file(Some(file_path), None, false).unwrap();
        assert!(body.contains("This is the body"));
        assert!(!body.contains("title:"));
    }

    #[test]
    fn test_get_metadata() {
        let mut file = NamedTempFile::new().unwrap();
        let content = "\
---
title: Metadata Test
tags: [rust, testing]
---
Document body here.
";
        write!(file, "{}", content).unwrap();
        let file_path = file.path().to_str().unwrap();

        let metadata = get_metadata(file_path).unwrap();
        assert!(metadata.is_some());
        let meta_str = metadata.unwrap();
        assert!(meta_str.contains("Metadata Test"));
    }
}
