// tests/config_integration.rs

use notemancy_core::config::{load_config, setup_test_config};
mod common;
use common::setup_test_env;
use std::env;
use std::error::Error;
use std::fs;
use tempfile::tempdir;

#[test]
fn integration_test_load_config_override() -> Result<(), Box<dyn Error>> {
    // reset_search()?;
    setup_test_env(100)?;
    let tmp_dir = tempdir()?;
    let config_dir = tmp_dir.path().join("gnosis");
    fs::create_dir_all(&config_dir)?;
    setup_test_config(&config_dir)?;

    env::set_var("GNOS_CONFIG_DIR", tmp_dir.path());
    let config = load_config()?;
    assert_eq!(config.general.unwrap().indicator.unwrap(), "notesy");

    let vaults = config.vaults.unwrap();
    assert!(vaults.contains_key("main"), "Vault 'main' not found");
    assert!(vaults.contains_key("work"), "Vault 'work' not found");

    env::remove_var("GNOS_CONFIG_DIR");
    Ok(())
}
