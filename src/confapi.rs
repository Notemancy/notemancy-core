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

/// Returns the configuration directory as a PathBuf.
///
/// On Windows, it returns a fixed path. On other systems, it uses the user’s home directory.
#[cfg(target_os = "windows")]
pub fn get_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NOTEMANCY_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    PathBuf::from("C:\\Users\\Default\\AppData\\Roaming\\notemancy")
}

#[cfg(not(target_os = "windows"))]
pub fn get_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NOTEMANCY_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    let home = dirs::home_dir().expect("Home directory not found");
    home.join(".config").join("notemancy")
}

/// Computes the full path to the config file.
pub fn get_config_file_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("ncy.yaml");
    path
}

/// Represents the whole configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub vault_dir: Option<PathBuf>,
    pub ai: Option<AIConfig>,
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

/// Checks whether the configuration file exists and validates its content.
///
/// - If the file does not exist, it creates an empty file and returns a `MissingConfig` error.
/// - If the file is empty, it returns an `EmptyConfig` error.
/// - Otherwise, it attempts to deserialize the file into a `Config` struct and
///   checks that required sections (e.g. the `ai` section and `vault_dir` field) are present.
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

    // Validate 'ai' section.
    if let Some(ai) = config.ai {
        if ai.semantic_thresh.is_none() {
            return Err(ConfigError::InvalidConfig(
                "Missing 'ai.semantic_thresh' field".into(),
            ));
        }
        if let Some(autotagging) = ai.autotagging {
            if autotagging.mode.is_none() {
                return Err(ConfigError::InvalidConfig(
                    "Missing 'ai.autotagging.mode' field".into(),
                ));
            }
        } else {
            return Err(ConfigError::InvalidConfig(
                "Missing 'ai.autotagging' section".into(),
            ));
        }
    } else {
        return Err(ConfigError::InvalidConfig("Missing 'ai' section".into()));
    }

    // Validate 'vault_dir' field.
    if config.vault_dir.is_none() {
        return Err(ConfigError::InvalidConfig(
            "Missing 'vault_dir' field".into(),
        ));
    }

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
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
        let (_temp_dir, _config_dir) = setup_temp_config_dir();
        // For testing purposes, refactor your functions to accept a custom config path.
        // Use something like: let result = validate_config_at(&config_dir.join("ncy.yaml"));
        // and assert that the error matches ConfigError::MissingConfig.
    }

    /// A test for `get_config` when the config file is empty.
    #[test]
    fn test_get_config_empty() {
        let (_temp_dir, config_dir) = setup_temp_config_dir();
        let config_path = config_dir.join("ncy.yaml");

        // Create an empty config file.
        fs::write(&config_path, "").expect("Failed to write empty config file");

        // For testing, use a function that accepts a config path.
        // e.g., let result = get_config_from(&config_path);
        // assert!(matches!(result, Err(ConfigError::EmptyConfig)));
    }
}
