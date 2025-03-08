use crate::confapi::get_config_dir;
use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;

/// Directory name for the database files.
pub const DB_DIR_NAME: &str = "database";
/// Database file name.
pub const DB_FILE_NAME: &str = "pagetable.sqlite";

/// Custom error type for the dbapi module.
#[derive(Debug)]
pub enum DbError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Io(e) => write!(f, "IO Error: {}", e),
            DbError::Sqlite(e) => write!(f, "SQLite Error: {}", e),
        }
    }
}

impl std::error::Error for DbError {}

impl From<std::io::Error> for DbError {
    fn from(err: std::io::Error) -> Self {
        DbError::Io(err)
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        DbError::Sqlite(err)
    }
}

pub fn get_db_file_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push(DB_DIR_NAME);
    path.push(DB_FILE_NAME);
    path
}

/// Checks that the database directory exists and that the SQLite file is present.
/// If the directory or file do not exist, they are created.
pub fn check_db_path() -> Result<(), DbError> {
    let db_file_path = get_db_file_path();
    if let Some(parent) = db_file_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    if !db_file_path.exists() {
        // Create an empty file.
        fs::File::create(&db_file_path)?;
    }
    Ok(())
}

/// Runs automatic migrations on the database.
/// First, it creates the `pagetable` table (if not present) with the new `project` column,
/// and then it checks if the `project` column exists in an already existing table and adds it if missing.
pub fn run_migrations() -> Result<(), DbError> {
    // Ensure the database path is set up.
    check_db_path()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;

    // Create the table if it does not exist.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pagetable (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            lpath TEXT UNIQUE NOT NULL,
            title TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            vpath TEXT NOT NULL,
            project TEXT
        )",
        [],
    )?;

    // Check if the 'project' column exists; if not, add it.
    let mut stmt = conn.prepare("PRAGMA table_info(pagetable)")?;
    let mut has_project = false;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let col_name: String = row.get("name")?;
        if col_name == "project" {
            has_project = true;
            break;
        }
    }
    if !has_project {
        // Note: ALTER TABLE ADD COLUMN in SQLite cannot use "IF NOT EXISTS" so we check beforehand.
        conn.execute("ALTER TABLE pagetable ADD COLUMN project TEXT", [])?;
    }

    Ok(())
}

/// A record to be inserted into the pagetable.
#[derive(Debug)]
pub struct Record {
    pub lpath: String,
    pub title: String,
    pub timestamp: String,
    pub vpath: String,
    /// New optional field.
    pub project: Option<String>,
}

/// Returned status for adding a record.
#[derive(Debug)]
pub enum AddRecordStatus {
    Inserted,
    AlreadyExists,
}

/// Inserts a new record into the pagetable.
/// If a record with the same `lpath` already exists, the function returns `AlreadyExists`.
pub fn add_record(record: &Record) -> Result<AddRecordStatus, DbError> {
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;
    let count = conn.execute(
        "INSERT OR IGNORE INTO pagetable (lpath, title, timestamp, vpath, project) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            record.lpath,
            record.title,
            record.timestamp,
            record.vpath,
            record.project
        ],
    )?;
    if count == 0 {
        Ok(AddRecordStatus::AlreadyExists)
    } else {
        Ok(AddRecordStatus::Inserted)
    }
}

/// Used to identify a record by its `id` or its `lpath`.
pub enum RecordIdentifier {
    Id(i64),
    Lpath(String),
}

/// A struct for providing optional updates to a record.
#[derive(Debug, Default)]
pub struct RecordUpdate {
    pub lpath: Option<String>,
    pub title: Option<String>,
    pub timestamp: Option<String>,
    pub vpath: Option<String>,
    /// New optional update field.
    pub project: Option<String>,
}

/// Updates a record in the `pagetable`.
/// The record is identified by either its `id` or `lpath`.
/// Only the fields provided (non-`None`) in `update` will be modified.
pub fn update_record(identifier: RecordIdentifier, update: RecordUpdate) -> Result<(), DbError> {
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;

    let mut query = "UPDATE pagetable SET ".to_string();
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(new_lpath) = update.lpath {
        clauses.push("lpath = ?");
        params.push(Box::new(new_lpath));
    }
    if let Some(new_title) = update.title {
        clauses.push("title = ?");
        params.push(Box::new(new_title));
    }
    if let Some(new_timestamp) = update.timestamp {
        clauses.push("timestamp = ?");
        params.push(Box::new(new_timestamp));
    }
    if let Some(new_vpath) = update.vpath {
        clauses.push("vpath = ?");
        params.push(Box::new(new_vpath));
    }
    if let Some(new_project) = update.project {
        clauses.push("project = ?");
        params.push(Box::new(new_project));
    }

    if clauses.is_empty() {
        // Nothing to update.
        return Ok(());
    }

    query.push_str(&clauses.join(", "));

    match identifier {
        RecordIdentifier::Id(id) => {
            query.push_str(" WHERE id = ?");
            params.push(Box::new(id));
        }
        RecordIdentifier::Lpath(lpath) => {
            query.push_str(" WHERE lpath = ?");
            params.push(Box::new(lpath));
        }
    }

    let params_slice: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    conn.execute(&query, params_slice.as_slice())?;
    Ok(())
}

/// Deletes a record from the `pagetable`.
/// The record is identified by either its `id` or its `lpath`.
pub fn delete_record(identifier: RecordIdentifier) -> Result<(), DbError> {
    run_migrations()?;
    let db_file_path = get_db_file_path();
    let conn = Connection::open(db_file_path)?;
    let (query, param): (&str, Box<dyn rusqlite::ToSql>) = match identifier {
        RecordIdentifier::Id(id) => ("DELETE FROM pagetable WHERE id = ?", Box::new(id)),
        RecordIdentifier::Lpath(lpath) => {
            ("DELETE FROM pagetable WHERE lpath = ?", Box::new(lpath))
        }
    };
    conn.execute(query, params![param])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::TempDir;

    // A helper function that returns an in-memory SQLite connection.
    fn get_in_memory_connection() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn test_run_migrations_in_memory() {
        let conn = get_in_memory_connection();
        // Run the migration query on the in-memory connection.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS pagetable (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                lpath TEXT UNIQUE NOT NULL,
                title TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                vpath TEXT NOT NULL,
                project TEXT
            )",
            [],
        )
        .expect("Migration failed");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='pagetable'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_check_db_path_temp_dir() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("notemancy");
        std::fs::create_dir_all(&config_dir).unwrap();
        // Here you might refactor `check_db_path` to accept a custom path for testing.
    }
}
