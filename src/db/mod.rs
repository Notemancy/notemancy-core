// src/db/mod.rs

use crate::config::get_config_dir;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub mod migrations;

/// A struct encapsulating database operations.
pub struct Database {
    db_path: PathBuf,
}

#[derive(Serialize)]
pub struct FileRecord {
    pub path: String,
    pub virtual_path: String,
    pub metadata: String,
}

impl Database {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let config_dir = get_config_dir()?;
        let db_dir = config_dir.join("db");
        if !db_dir.exists() {
            fs::create_dir_all(&db_dir)?;
            println!("Created database directory: {:?}", db_dir);
        }
        let db_path = db_dir.join("database.sqlite");

        // Create the Database instance
        let db = Database { db_path };

        // Check if the database file exists and has tables
        let should_initialize = if !db.db_path.exists() {
            true // New database needs initialization
        } else {
            // Check if tables exist
            let conn = db.connect()?;
            let mut stmt = conn.prepare(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('pagetable', 'attachments')"
        )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count < 2 // If we don't have both tables, we need to initialize
        };

        // Run migrations if needed
        if should_initialize {
            db.setup()?;
        }

        Ok(db)
    }

    /// Opens a new connection to the database.
    pub fn connect(&self) -> Result<Connection, Box<dyn Error>> {
        let conn = Connection::open(&self.db_path)?;
        Ok(conn)
    }

    /// Sets up the database by running migrations.
    pub fn setup(&self) -> Result<(), Box<dyn Error>> {
        let conn = self.connect()?;
        migrations::run_migrations(&conn)?;
        println!("Database setup completed at: {:?}", &self.db_path);
        Ok(())
    }

    /// Inserts (or updates) a page (note) into the `pagetable`.
    pub fn add_page(
        &self,
        vault: &str,
        path: &str,
        virtual_path: &str,
        metadata: &str,
        last_modified: &str,
        created: &str,
    ) -> Result<(), Box<dyn Error>> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO pagetable (vault, path, virtualPath, metadata, last_modified, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
               virtualPath=excluded.virtualPath,
               metadata=excluded.metadata,
               last_modified=excluded.last_modified,
               created=excluded.created",
            params![vault, path, virtual_path, metadata, last_modified, created],
        )?;
        Ok(())
    }

    /// Inserts (or updates) an attachment into the `attachments` table.
    pub fn add_attachment(
        &self,
        local_path: &str,
        virtual_path: &str,
        file_type: &str,
    ) -> Result<(), Box<dyn Error>> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO attachments (path, virtualPath, type)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
               virtualPath=excluded.virtualPath,
               type=excluded.type",
            params![local_path, virtual_path, file_type],
        )?;
        Ok(())
    }

    /// Queries the `pagetable` selecting the user-specified columns from all rows.
    ///
    /// Returns a vector of hash maps where each map represents a row with the column names
    /// as keys and the corresponding values.
    pub fn query_by_fields(
        &self,
        fields: &[&str],
    ) -> Result<Vec<HashMap<String, String>>, Box<dyn Error>> {
        let conn = self.connect()?;
        let columns = fields.join(", ");
        let sql = format!("SELECT {} FROM pagetable", columns);
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let mut map = HashMap::new();
            for (i, field) in fields.iter().enumerate() {
                let val: String = row.get(i)?;
                map.insert(field.to_string(), val);
            }
            results.push(map);
        }
        Ok(results)
    }

    /// Prints statistics (the note count) for the specified vault.
    pub fn print_stats(&self, vault: &str) -> Result<(), Box<dyn Error>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM pagetable WHERE vault = ?1")?;
        let count: i64 = stmt.query_row(params![vault], |row| row.get(0))?;
        println!("Vault '{}' has {} notes.", vault, count);
        Ok(())
    }

    /// Cleans up stale records from the `pagetable`.
    ///
    /// A record is considered stale if the file at its stored path no longer exists.
    pub fn cleanup_stale_records(&self) -> Result<(), Box<dyn Error>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id, path FROM pagetable")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            Ok((id, path))
        })?;
        for row in rows {
            let (id, path) = row?;
            if !Path::new(&path).exists() {
                conn.execute("DELETE FROM pagetable WHERE id = ?1", params![id])?;
                println!("Deleted stale record id {}: {}", id, path);
            }
        }
        Ok(())
    }

    /// Lists all file paths stored in the `pagetable` for the given vault.
    pub fn list_files(&self, vault: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT path FROM pagetable WHERE vault = ?1")?;
        let mut rows = stmt.query(params![vault])?;
        println!("Files in vault '{}':", vault);
        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            println!("{}", path);
            results.push(path);
        }
        Ok(results)
    }

    pub fn get_file_tree(&self) -> Result<Vec<FileRecord>, Box<dyn Error>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT path, virtualPath, metadata FROM pagetable ORDER BY virtualPath ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                virtual_path: row.get(1)?,
                metadata: row.get(2)?,
            })
        })?;

        let mut results = Vec::new();
        for record in rows {
            results.push(record?);
        }
        Ok(results)
    }

    pub fn get_page_by_virtual_path(
        &self,
        virtual_path: &str,
    ) -> Result<Option<FileRecord>, Box<dyn Error>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT path, virtualPath, metadata FROM pagetable WHERE virtualPath = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query([virtual_path])?;
        if let Some(row) = rows.next()? {
            let record = FileRecord {
                path: row.get(0)?,
                virtual_path: row.get(1)?,
                metadata: row.get(2)?,
            };
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }
}

/// Convenience function that sets up the database if it doesn't exist.
/// This creates the necessary directory and database file, then runs migrations.
pub fn setup_database() -> Result<(), Box<dyn Error>> {
    let db = Database::new()?;
    // db.setup()?;
    Ok(())
}
