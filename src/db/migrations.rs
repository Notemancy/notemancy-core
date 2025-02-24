// src/db/migrations.rs

use rusqlite::Connection;
use std::error::Error;

/// Runs the necessary SQL migrations to set up the database schema.
///
/// Creates the `pagetable` for notes and the `attachments` table if they do not already exist.
pub fn run_migrations(conn: &Connection) -> Result<(), Box<dyn Error>> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS pagetable (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            vault TEXT NOT NULL,
            path TEXT UNIQUE,
            virtualPath TEXT,
            metadata TEXT,
            last_modified TEXT,
            created TEXT
        );
        CREATE TABLE IF NOT EXISTS attachments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT UNIQUE,
            virtualPath TEXT,
            type TEXT
        );
        ",
    )?;
    Ok(())
}
