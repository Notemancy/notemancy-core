use ignore::WalkBuilder;
use rayon::prelude::*;
use serde_json;
use serde_yaml;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Represents a file that was scanned from a vault.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub vault: String,
    pub local_path: PathBuf,
    pub virtual_path: String,
    pub metadata: Option<serde_json::Value>,
    pub last_modified: String,
    pub created: String,
}

/// A scanning interface which holds vault names, associated paths, and an indicator string.
pub struct Scanner {
    vaults: Vec<(String, Vec<PathBuf>)>,
    indicator: String,
}

impl Scanner {
    /// Constructs a new `Scanner` from the given vaults and indicator.
    pub fn new(vaults: Vec<(String, Vec<PathBuf>)>, indicator: String) -> Self {
        Scanner { vaults, indicator }
    }

    /// Loads configuration from the config module and returns a `Scanner` instance.
    /// If no explicit vaults are provided, it will use the vault marked as default (via
    /// `default: true` in the config) and its associated paths. If no default vault is found,
    /// then all vaults are used.
    pub fn from_config() -> Result<Self, Box<dyn Error>> {
        let config = crate::config::load_config()?;
        let indicator = config
            .general
            .as_ref()
            .and_then(|g| g.indicator.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("notesy")
            .to_string();

        let mut vaults = Vec::new();
        if let Some(vaults_config) = config.vaults {
            // Try to find vaults that are marked as default.
            for (vault_name, vault_props) in vaults_config.iter() {
                if vault_props.default.unwrap_or(false) {
                    if let Some(paths) = &vault_props.paths {
                        let pathbufs = paths.iter().map(PathBuf::from).collect();
                        vaults.push((vault_name.clone(), pathbufs));
                    }
                }
            }
            // If no vault is marked as default, then use all vaults.
            if vaults.is_empty() {
                for (vault_name, vault_props) in vaults_config.iter() {
                    if let Some(paths) = &vault_props.paths {
                        let pathbufs = paths.iter().map(PathBuf::from).collect();
                        vaults.push((vault_name.clone(), pathbufs));
                    }
                }
            }
        }
        Ok(Scanner { vaults, indicator })
    }

    pub fn scan_markdown_files(&self) -> Result<(Vec<ScannedFile>, String), Box<dyn Error>> {
        // Wrap the DB in an Arc<Mutex<>> if it's not thread-safe.
        let db = Arc::new(Mutex::new(crate::db::Database::new()?));

        // Collect all file tasks from all vaults into a vector of (vault, file_path) pairs.
        let tasks: Vec<(String, PathBuf)> = self
            .vaults
            .iter()
            .flat_map(|(vault, paths)| {
                paths.iter().flat_map(move |vault_path| {
                    list_files_with_extension(vault_path, &self.indicator, &["md", "markdown"])
                        .into_iter()
                        .map(move |file| (vault.clone(), file))
                })
            })
            .collect();

        // Process files in parallel using Rayon.
        let results: Vec<_> = tasks
            .par_iter()
            .map(|(vault, file)| {
                match process_file(file, &self.indicator, vault) {
                    Ok(mut sf) => {
                        sf.vault = vault.clone();
                        let metadata_str = sf
                            .metadata
                            .as_ref()
                            .map_or(String::new(), |m| m.to_string());
                        // Lock the DB for thread-safe access.
                        let db_lock = db.lock().unwrap();
                        match db_lock.add_page(
                            vault,
                            &sf.local_path.to_string_lossy(),
                            &sf.virtual_path,
                            &metadata_str,
                            &sf.last_modified,
                            &sf.created,
                        ) {
                            Ok(()) => Ok(sf),
                            Err(e) => Err((file.clone(), format!("DB insert error: {}", e))),
                        }
                    }
                    Err(e) => Err((file.clone(), format!("Processing error: {}", e))),
                }
            })
            .collect();

        // Separate successful scans and errors.
        let mut scanned_files = Vec::new();
        let mut errors = Vec::<(PathBuf, String)>::new();
        for res in results {
            match res {
                Ok(sf) => scanned_files.push(sf),
                Err(err) => errors.push(err),
            }
        }

        // Build a summary string.
        let mut summary = String::new();
        if !errors.is_empty() {
            summary.push_str("The following errors occurred during markdown scanning:\n");
            for (path, msg) in &errors {
                summary.push_str(&format!("File {:?}: {}\n", path, msg));
            }
        } else {
            summary.push_str("No errors during markdown scanning.\n");
        }

        let mut vault_summary = std::collections::HashMap::new();
        for sf in &scanned_files {
            *vault_summary.entry(sf.vault.clone()).or_insert(0) += 1;
        }
        summary.push_str("\nMarkdown scanning summary:\n");
        for (vault, count) in vault_summary {
            summary.push_str(&format!(
                "Vault {}: {} markdown files scanned.\n",
                vault, count
            ));
        }

        Ok((scanned_files, summary))
    }

    /// Scans for image files in all vaults.
    ///
    /// For each vault and each associated path, this method walks the directory,
    /// filters for files with allowed image extensions that contain the configured indicator,
    /// processes each file, and inserts it into the database as an attachment.
    pub fn scan_images(&self) -> Result<(), Box<dyn Error>> {
        let db = crate::db::Database::new()?;
        let allowed_exts = ["png", "jpg", "jpeg", "gif", "webp", "svg"];
        let mut scanned_files = Vec::new();
        let mut errors = Vec::<(PathBuf, String)>::new();

        for (vault, paths) in &self.vaults {
            for vault_path in paths {
                let files = list_files_with_extension(vault_path, &self.indicator, &allowed_exts);
                for file in files {
                    match process_file(&file, &self.indicator, vault) {
                        Ok(mut sf) => {
                            sf.vault = vault.clone();
                            if let Err(e) = db.add_attachment(
                                &sf.local_path.to_string_lossy(),
                                &sf.virtual_path,
                                "image",
                            ) {
                                errors.push((file.clone(), format!("DB insert error: {}", e)));
                            }
                            scanned_files.push(sf);
                        }
                        Err(e) => {
                            errors.push((file.clone(), format!("Processing error: {}", e)));
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            println!("\nThe following errors occurred during image scanning:");
            for (path, msg) in &errors {
                println!("File {:?}: {}", path, msg);
            }
        } else {
            println!("\nNo errors during image scanning.");
        }

        let mut summary = HashMap::new();
        for sf in &scanned_files {
            *summary.entry(sf.vault.clone()).or_insert(0) += 1;
        }
        println!("\nImage scanning summary:");
        for (vault, count) in summary {
            println!("Vault {}: {} images scanned.", vault, count);
        }

        Ok(())
    }
}

/// Helper function: Returns the path components *after* the first occurrence of `indicator` in `file_path`.
fn extract_relative_path_after_indicator(file_path: &Path, indicator: &str) -> Option<PathBuf> {
    let mut found = false;
    let mut rel_components = Vec::new();

    for comp in file_path.components() {
        if found {
            rel_components.push(comp.as_os_str());
        } else if let Component::Normal(os_str) = comp {
            if os_str == indicator {
                found = true;
            }
        }
    }
    if found {
        Some(rel_components.iter().collect())
    } else {
        None
    }
}

/// Helper function: Extracts YAML frontmatter from a file (if present).
fn extract_yaml_frontmatter(
    file_path: &Path,
) -> Result<Option<serde_yaml::Mapping>, Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let mut lines = content.lines();
    if let Some(first_line) = lines.next() {
        if first_line.trim() == "---" {
            let mut fm_lines = Vec::new();
            for line in lines {
                if line.trim() == "---" {
                    break;
                }
                fm_lines.push(line);
            }
            let fm_str = fm_lines.join("\n");
            let mapping: serde_yaml::Mapping = serde_yaml::from_str(&fm_str)?;
            return Ok(Some(mapping));
        }
    }
    Ok(None)
}

/// Helper function: Processes a file (markdown or image) by extracting relative path,
/// retrieving metadata, and optionally adjusting the virtual path via YAML frontmatter.
fn process_file(
    file_path: &Path,
    indicator: &str,
    vault: &str,
) -> Result<ScannedFile, Box<dyn Error>> {
    let rel_path =
        extract_relative_path_after_indicator(file_path, indicator).ok_or_else(|| {
            format!(
                "Indicator '{}' not found in path {:?}",
                indicator, file_path
            )
        })?;
    let mut virtual_path = rel_path.to_string_lossy().to_string();
    let meta = fs::metadata(file_path)?;
    let modified_time = meta.modified()?;
    let created_time = meta.created().unwrap_or(modified_time);
    let modified_str = format!("{:?}", modified_time);
    let created_str = format!("{:?}", created_time);

    let frontmatter = extract_yaml_frontmatter(file_path).unwrap_or(None);
    if let Some(ref mapping) = frontmatter {
        if let Some(folder_value) = mapping.get(serde_yaml::Value::String("folder".to_string())) {
            if let Some(folder_str) = folder_value.as_str() {
                virtual_path = format!("{}/{}", folder_str.trim_end_matches('/'), virtual_path);
            }
        }
    }
    let metadata_json = if let Some(mapping) = frontmatter {
        Some(serde_json::to_value(mapping)?)
    } else {
        None
    };

    Ok(ScannedFile {
        vault: vault.to_string(),
        local_path: file_path.to_owned(),
        virtual_path,
        metadata: metadata_json,
        last_modified: modified_str,
        created: created_str,
    })
}

/// Helper function: Walks the given directory and returns all files that:
///   - Have an extension matching one in `allowed_exts` (case-insensitive)
///   - Contain the provided indicator in their path.
fn list_files_with_extension(
    vault_path: &Path,
    indicator: &str,
    allowed_exts: &[&str],
) -> Vec<PathBuf> {
    let walker = WalkBuilder::new(vault_path).build();
    walker
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                if e.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    if let Some(ext) = e.path().extension().and_then(|s| s.to_str()) {
                        let ext_lower = ext.to_lowercase();
                        if allowed_exts.contains(&ext_lower.as_str())
                            && extract_relative_path_after_indicator(e.path(), indicator).is_some()
                        {
                            return Some(e.path().to_owned());
                        }
                    }
                }
                None
            })
        })
        .collect()
}
