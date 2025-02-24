use color_eyre::Report;
use color_eyre::Result;
use meilisearch_sdk::client::Client;
use meilisearch_sdk::search::Selectors;
use portpicker::pick_unused_port;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The master key to be used consistently.
const MASTER_KEY: &str = "ankliemb01923en498cndeu1948mdowld";

/// A helper struct to spawn and manage a MeiliSearch server process.
pub struct MeiliSearchServer {
    process: Option<Child>,
    /// The port that MeiliSearch is listening on.
    pub port: u16,
}

impl MeiliSearchServer {
    pub fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let out_dir = env!("OUT_DIR");
        let binary_path = PathBuf::from(out_dir).join("meilisearch");

        let port = pick_unused_port().ok_or("No unused port available")?;
        let port_arg = format!("127.0.0.1:{}", port);

        // Redirect stdout and stderr to null so that logs don't print to your terminal.
        let child = Command::new(binary_path)
            .arg("--master-key")
            .arg(MASTER_KEY)
            .arg("--http-addr")
            .arg(&port_arg)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        std::thread::sleep(Duration::from_secs(1));

        Ok(Self {
            process: Some(child),
            port,
        })
    }

    /// Shuts down the MeiliSearch server.
    pub fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(child) = self.process.as_mut() {
            child.kill()?;
            child.wait()?;
        }
        self.process = None;
        Ok(())
    }
}

/// A simple document type to be indexed in MeiliSearch.
#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub path: String,
    pub content: String,
}

/// The main search interface.
pub struct SearchInterface {
    client: Client,
}

impl SearchInterface {
    /// Creates a new search interface using the provided base URL.
    pub fn new_with_url(url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = Client::new(url, Some(MASTER_KEY))?;
        Ok(Self { client })
    }

    pub fn new_from_server(server: &MeiliSearchServer) -> Result<Self, Box<dyn std::error::Error>> {
        let url = format!("http://127.0.0.1:{}", server.port);
        Self::new_with_url(&url)
    }

    /// Creates a new search interface using the default URL "http://127.0.0.1:7700".
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_with_url("http://127.0.0.1:7700")
    }

    /// Indexes the provided files.
    pub async fn index_files(&self, paths: Vec<PathBuf>) -> Result<()> {
        let index = self.client.index("gnosis");

        index
            .set_searchable_attributes(&["id", "content", "path"])
            .await?;
        index
            .set_displayed_attributes(&["id", "path", "content"])
            .await?;

        let mut documents = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            if let Ok(content) = fs::read_to_string(path) {
                documents.push(Document {
                    id: i.to_string(),
                    path: path.to_string_lossy().to_string(),
                    content,
                });
            }
        }

        let task = index.add_documents(&documents, Some("id")).await?;
        task.wait_for_completion(&self.client, None, None).await?;
        Ok(())
    }

    /// Performs a search with the given query.
    pub async fn search(&self, query: &str) -> Result<Vec<Document>> {
        let index = self.client.index("gnosis");
        let results = index
            .search()
            .with_query(query)
            .with_attributes_to_retrieve(Selectors::Some(&["id", "path", "content"]))
            .execute::<Document>()
            .await?;
        let documents = results.hits.into_iter().map(|hit| hit.result).collect();
        Ok(documents)
    }

    /// **High-level helper:** Creates a default search interface and performs a search.
    ///
    /// This method wraps the creation of the interface and the search query,
    /// so that your UI code can simply call this function.
    pub async fn search_documents(query: &str) -> color_eyre::Result<Vec<Document>> {
        let si = SearchInterface::new().map_err(|e| Report::msg(e.to_string()))?;
        si.search(query).await
    }
}
