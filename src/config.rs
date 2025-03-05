// src/config.rs
#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

// src/config.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub indicator: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultProperties {
    #[serde(default)]
    pub default: Option<bool>,
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AIConfig {
    pub model_name: Option<String>,
    pub initial_capacity: Option<usize>,
    #[serde(default = "default_ef_construction")]
    pub ef_construction: usize,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

fn default_ef_construction() -> usize {
    800
}

fn default_max_connections() -> usize {
    24
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub general: Option<GeneralConfig>,
    pub vaults: Option<HashMap<String, VaultProperties>>,
    pub ai: Option<AIConfig>, // Added AI configuration
}

impl Default for Config {
    fn default() -> Self {
        let mut vaults = HashMap::new();
        vaults.insert(
            "main".into(),
            VaultProperties {
                default: Some(true),
                paths: None,
            },
        );
        Self {
            general: Some(GeneralConfig {
                indicator: Some("notesy".into()),
            }),
            vaults: Some(vaults),
            ai: Some(AIConfig {
                model_name: Some("all-MiniLM-L6-v2".into()),
                initial_capacity: Some(10000),
                ef_construction: default_ef_construction(),
                max_connections: default_max_connections(),
            }),
        }
    }
}

/// Returns the configuration directory.
/// If the environment variable `GNOS_CONFIG_DIR` is set, use that value joined with "gnosis".
/// Otherwise, use the system default.
pub fn get_config_dir() -> Result<PathBuf, Box<dyn Error>> {
    let config_dir = if let Ok(dir) = std::env::var("GNOS_CONFIG_DIR") {
        PathBuf::from(dir).join("gnosis")
    } else {
        let base_config_dir = dirs::config_dir().ok_or("Could not determine config directory")?;
        base_config_dir.join("gnosis")
    };
    // For debugging, print out the config directory:
    // eprintln!("Using config directory: {:?}", config_dir);
    Ok(config_dir)
}

/// Loads the configuration from config.yaml in the gnosis config directory.
pub fn load_config() -> Result<Config, Box<dyn Error>> {
    let config_dir = get_config_dir()?;
    let config_file = config_dir.join("config.yaml");
    let contents = fs::read_to_string(config_file)?;
    let config: Config = serde_yaml::from_str(&contents)?;
    Ok(config)
}

/// Returns the full path to the configuration file (config.yaml)
pub fn get_config_file_path() -> Result<PathBuf, Box<dyn Error>> {
    Ok(get_config_dir()?.join("config.yaml"))
}

/// Sets up the configuration by creating the config folder and file if they don’t exist.
pub fn setup_config() -> Result<(), Box<dyn Error>> {
    let config_dir = get_config_dir()?;
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
        println!("Created configuration directory: {:?}", config_dir);
    }

    let config_file = config_dir.join("config.yaml");
    if !config_file.exists() {
        // Write a default configuration to config.yaml.
        let default_config = Config::default();
        let yaml_str = serde_yaml::to_string(&default_config)?;
        fs::write(&config_file, yaml_str)?;
        println!("Created default configuration file at: {:?}", config_file);
    }
    Ok(())
}

/// Opens the configuration file in the user's preferred editor.
pub fn open_config_in_editor() -> Result<(), Box<dyn Error>> {
    let config_file = get_config_file_path()?;

    if let Ok(editor) = std::env::var("EDITOR") {
        let status = std::process::Command::new(editor)
            .arg(&config_file)
            .status()?;
        if !status.success() {
            return Err("Editor exited with an error".into());
        }
    } else {
        #[cfg(target_os = "macos")]
        {
            let status = std::process::Command::new("open")
                .arg(&config_file)
                .status()?;
            if !status.success() {
                return Err("Failed to open config file with 'open' command".into());
            }
        }
        #[cfg(target_os = "linux")]
        {
            let status = std::process::Command::new("xdg-open")
                .arg(&config_file)
                .status()?;
            if !status.success() {
                return Err("Failed to open config file with 'xdg-open' command".into());
            }
        }
        #[cfg(target_os = "windows")]
        {
            let status = std::process::Command::new("cmd")
                .args(&["/C", "start", "", config_file.to_str().unwrap()])
                .status()?;
            if !status.success() {
                return Err("Failed to open config file with 'start' command".into());
            }
        }
    }
    Ok(())
}

/// For testing, sets up a test configuration in the provided directory.
#[allow(dead_code)]
pub fn setup_test_config(test_dir: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(test_dir)?;
    let config_file = test_dir.join("config.yaml");
    let config_content = r#"general:
  indicator: "notesy"
vaults:
  main:
    default: true
    paths:
      - "path/to/test_vault/main"
  work:
    paths:
      - "path/to/test_vault/work"
ai:
  model_path: "path/to/test/model"
  initial_capacity: 1000
  ef_construction: 800
  max_connections: 24
"#;
    fs::write(config_file, config_content)?;
    Ok(())
}
