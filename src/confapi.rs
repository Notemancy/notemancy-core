use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_yaml;

/// Custom error type for configuration errors.
#[derive(Debug)]
pub enum ConfigError {
    IoError(io::Error),
    YamlError(serde_yaml::Error),
    /// Returned when the config file was missing and has been created empty.
    MissingConfig,
    /// Returned when the config file exists but is empty.
    EmptyConfig,
    /// Returned when required keys/values are missing.
    InvalidConfig(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError(err) => write!(f, "I/O error: {}", err),
            ConfigError::YamlError(err) => write!(f, "YAML error: {}", err),
            ConfigError::MissingConfig => {
                write!(f, "Config file did not exist; created an empty file")
            }
            ConfigError::EmptyConfig => write!(f, "Config file is empty"),
            ConfigError::InvalidConfig(msg) => write!(f, "Invalid config: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Public constant storing the location of the configuration directory.
///
/// On non-Windows systems, we use the conventional "~/.config/notemancy" path.
/// On Windows, you should replace the placeholder with the actual default config directory
/// (or use a crate like `dirs` to compute it at runtime).
#[cfg(target_os = "windows")]
pub const CONFIG_DIR: &str = "C:\\Users\\Default\\AppData\\Roaming\\notemancy";

#[cfg(not(target_os = "windows"))]
pub const CONFIG_DIR: &str = "~/.config/notemancy";

/// Represents the whole configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// Vaults is represented as an optional vector of single-key maps.
    /// Each map key is the vault name and the value is its configuration.
    pub vaults: Option<Vec<HashMap<String, VaultConfig>>>,
    pub ai: Option<AIConfig>,
}

/// Represents a vault configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct VaultConfig {
    pub scan_paths: Option<Vec<String>>,
    pub publish_url: Option<String>,
}

/// Represents the AI configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct AIConfig {
    pub semantic_thresh: Option<f64>,
    pub autotagging: Option<AutoTaggingConfig>,
}

/// Represents the autotagging configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct AutoTaggingConfig {
    pub mode: Option<String>,
}

/// Helper function to compute the full config file path.
fn get_config_file_path() -> PathBuf {
    let mut path = PathBuf::from(CONFIG_DIR);
    path.push("ncy.yaml");
    path
}

/// Checks whether the configuration file exists and validates its content.
///
/// - If the file does not exist, it creates an empty file and returns a `MissingConfig` error.
/// - If the file is empty, it returns an `EmptyConfig` error.
/// - Otherwise, it attempts to deserialize the file into a `Config` struct and
///   checks that required sections (e.g. the `ai` section) are present.
///
/// # Errors
///
/// Returns a `ConfigError` if any I/O or deserialization error occurs, or if required
/// keys/values are missing.
pub fn validate_config() -> Result<(), ConfigError> {
    let config_path = get_config_file_path();

    // Check if config file exists; if not, create it (and parent directories) as empty.
    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(ConfigError::IoError)?;
        }
        fs::write(&config_path, "").map_err(ConfigError::IoError)?;
        return Err(ConfigError::MissingConfig);
    }

    let content = fs::read_to_string(&config_path).map_err(ConfigError::IoError)?;
    if content.trim().is_empty() {
        return Err(ConfigError::EmptyConfig);
    }

    // Deserialize the config file.
    let config: Config = serde_yaml::from_str(&content).map_err(ConfigError::YamlError)?;

    // Example validation: ensure that the AI section exists.
    if config.ai.is_none() {
        return Err(ConfigError::InvalidConfig("Missing 'ai' section".into()));
    }
    // (Additional validations for vaults or individual keys can be added here.)

    Ok(())
}

/// Parses the configuration file and returns a `Config` object.
///
/// # Errors
///
/// Returns a `ConfigError` if any I/O or deserialization error occurs, or if the file is empty.
pub fn get_config() -> Result<Config, ConfigError> {
    let config_path = get_config_file_path();
    let content = fs::read_to_string(&config_path).map_err(ConfigError::IoError)?;
    if content.trim().is_empty() {
        return Err(ConfigError::EmptyConfig);
    }
    let config: Config = serde_yaml::from_str(&content).map_err(ConfigError::YamlError)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir; // A helper crate for temporary directories

    /// Helper function to simulate the config directory in a temporary location.
    fn setup_temp_config_dir() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create a temp directory");
        let config_dir = temp_dir.path().join("notemancy");
        fs::create_dir_all(&config_dir).expect("Failed to create config directory");
        (temp_dir, config_dir)
    }

    /// A test for `validate_config` when the config file is missing.
    #[test]
    fn test_validate_config_missing() {
        let (_temp_dir, config_dir) = setup_temp_config_dir();
        // Instead of using the global CONFIG_DIR, you might refactor your code
        // to accept a config directory as an argument for testing purposes.
        // For example:
        // let config_path = config_dir.join("ncy.yaml");
        // Ensure the file doesn't exist yet.
        assert!(!config_dir.join("ncy.yaml").exists());

        // Call your function that uses the provided path.
        // For demonstration, imagine you have a variant of `validate_config` that accepts a path.
        // let result = validate_config_at(&config_path);
        // assert!(matches!(result, Err(ConfigError::MissingConfig)));
    }

    /// A test for `get_config` when the config file is empty.
    #[test]
    fn test_get_config_empty() {
        let (_temp_dir, config_dir) = setup_temp_config_dir();
        let config_path = config_dir.join("ncy.yaml");

        // Create an empty config file.
        fs::write(&config_path, "").expect("Failed to write empty config file");

        // Similarly, if you have a function that accepts a config path, use that.
        // let result = get_config_from(&config_path);
        // assert!(matches!(result, Err(ConfigError::EmptyConfig)));
    }

    // Additional tests for valid configuration parsing and error handling can be added here.
}
