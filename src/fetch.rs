use crate::db::Database;
use crate::db::FileRecord;
use mime_guess::from_path;
use std::error::Error;
use std::fs;

/// Structure to hold both the content and metadata of a page.
pub struct PageContent {
    pub content: String,
    pub metadata: String,
}

/// The main interface for retrieving pages and attachments.
pub struct Fetch {
    db: Database,
}

impl Fetch {
    /// Creates a new instance of `Fetch`.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let db = Database::new()?;
        Ok(Fetch { db })
    }

    pub fn get_file_tree(&self) -> Result<Vec<FileRecord>, Box<dyn Error>> {
        self.db.get_file_tree()
    }

    /// Sets up the database (runs migrations, etc.).
    pub fn setup(&self) -> Result<(), Box<dyn Error>> {
        self.db.setup()
    }

    /// Retrieves the content of a page (markdown file) and its metadata by its virtual path.
    ///
    /// This method queries the `pagetable` for a record matching the provided virtual path.
    /// If found, it uses the stored local path to read the file contents and returns both the content
    /// and the metadata.
    pub fn get_page_content(&self, virtual_path: &str) -> Result<PageContent, Box<dyn Error>> {
        let conn = self.db.connect()?;
        // Now selecting both the local path and metadata.
        let mut stmt =
            conn.prepare("SELECT path, metadata FROM pagetable WHERE virtualPath = ?1")?;
        let mut rows = stmt.query([virtual_path])?;

        if let Some(row) = rows.next()? {
            let local_path: String = row.get(0)?;
            let metadata: String = row.get(1)?;
            println!("meta {}", metadata);
            let content = fs::read_to_string(&local_path)?;
            Ok(PageContent { content, metadata })
        } else {
            Err(format!("No page found with virtual path: {}", virtual_path).into())
        }
    }

    pub fn get_attachment_content(
        &self,
        virtual_path: &str,
    ) -> Result<(Vec<u8>, String), Box<dyn Error>> {
        let conn = self.db.connect()?;
        let mut stmt = conn.prepare("SELECT path FROM attachments WHERE virtualPath = ?1")?;
        let mut rows = stmt.query([virtual_path])?;
        if let Some(row) = rows.next()? {
            let local_path: String = row.get(0)?;
            // Read raw bytes instead of string
            let content = fs::read(&local_path)?;
            // Guess the MIME type from the file extension
            let content_type = from_path(&local_path).first_or_octet_stream().to_string();

            Ok((content, content_type))
        } else {
            Err(format!("No attachment found with virtual path: {}", virtual_path).into())
        }
    }
}
