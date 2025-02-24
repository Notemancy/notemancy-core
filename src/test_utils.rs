// use chrono::Utc;
use crate::db;
use rand::seq::SliceRandom;
use rand::Rng;
use reqwest::blocking;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

pub fn get_metaphorpsum_text() -> Result<String, Box<dyn Error>> {
    let url = "http://metaphorpsum.com/paragraphs/5/5";
    let response = blocking::get(url)?;
    Ok(response.text()?)
}

// pub fn get_bible_verse() -> Result<String, Box<dyn Error>> {
//     let url = "https://bible-api.com/data/web/random";
//     let response = blocking::get(url)?;
//     let json: serde_json::Value = response.json()?;
//     if let Some(text) = json.get("text").and_then(|t| t.as_str()) {
//         Ok(text.to_string())
//     } else {
//         Err("Bible API did not return a verse".into())
//     }
// }

// pub fn get_spaceflight_article() -> Result<String, Box<dyn Error>> {
//     let mut rng = rand::thread_rng();
//     let id: u32 = rng.gen_range(1..1000);
//     let url = format!("https://api.spaceflightnewsapi.net/v4/articles/{}", id);
//     let response = blocking::get(&url)?;
//     let json: serde_json::Value = response.json()?;
//     let title = json
//         .get("title")
//         .and_then(|t| t.as_str())
//         .unwrap_or("No Title");
//     let summary = json
//         .get("summary")
//         .and_then(|s| s.as_str())
//         .unwrap_or("No Summary");
//     Ok(format!("{}: {}", title, summary))
// }

pub fn get_random_content() -> String {
    let mut rng = rand::thread_rng();
    match rng.gen_range(0..3) {
        0 => get_metaphorpsum_text().unwrap_or_else(|_| "Default Metaphorpsum text.".to_string()),
        1 => get_metaphorpsum_text().unwrap_or_else(|_| "Default Metaphorpsum text.".to_string()),
        2 => get_metaphorpsum_text().unwrap_or_else(|_| "Default Metaphorpsum text.".to_string()),
        _ => "Default content.".to_string(),
    }
}

pub fn generate_test_images(vault_root: &Path, count: usize) -> Result<(), Box<dyn Error>> {
    println!("Generating {} test images in {:?}", count, vault_root);
    let start = Instant::now();
    let images_dir = vault_root.join("images");
    fs::create_dir_all(&images_dir)?;
    for i in 1..=count {
        let file_path = images_dir.join(format!("image_{}.png", i));
        // Write PNG header bytes as dummy content.
        File::create(&file_path)?.write_all(b"\x89PNG\r\n\x1a\n")?;
        if i % 5 == 0 {
            println!("Generated {} images...", i);
        }
    }
    println!("Finished generating images in {:?}", start.elapsed());
    Ok(())
}

pub fn generate_test_vault(vault_root: &Path, note_count: usize) -> Result<(), Box<dyn Error>> {
    println!(
        "Generating test vault at {:?} with {} notes...",
        vault_root, note_count
    );
    let start = Instant::now();
    fs::create_dir_all(vault_root)?;

    // Prepare note titles and also store each note’s virtual path.
    let mut titles = Vec::new();
    let mut virtual_paths = Vec::new();
    for i in 1..=note_count {
        let title = format!("Test Note {}", i);
        titles.push(title);
        // In this simple case, the virtual path is the filename.
        virtual_paths.push(format!("note_{}.md", i));
    }

    let mut rng = rand::thread_rng();
    for i in 0..note_count {
        let title = &titles[i];
        let file_name = &virtual_paths[i];
        let file_path = vault_root.join(file_name);
        let content = get_random_content();
        // Frontmatter now contains only the title.
        let frontmatter = format!("---\ntitle: \"{}\"\n---\n\n", title);
        let mut body = format!("{}{}", frontmatter, content);
        let add_links = if i < 100 { true } else { rng.gen_bool(0.3) };
        if add_links {
            // Instead of linking by title, choose some other notes’ virtual paths.
            // Exclude the current note.
            let mut other_virtuals: Vec<&String> =
                virtual_paths.iter().filter(|vp| *vp != file_name).collect();
            other_virtuals.shuffle(&mut rng);
            let link_count = rng.gen_range(1..=3);
            let links: Vec<String> = other_virtuals
                .into_iter()
                .take(link_count)
                .map(|vp| format!("[[{}]]", vp))
                .collect();
            body.push_str("\n\nWiki Links: ");
            body.push_str(&links.join(" "));
        }
        let mut file = File::create(file_path)?;
        file.write_all(body.as_bytes())?;
        if (i + 1) % 10 == 0 {
            println!("Generated note {}/{}", i + 1, note_count);
        }
    }

    // Create the "home.md" note with random content.
    let home_title = "Home";
    let home_file_path = vault_root.join("home.md");
    let home_content = get_random_content();
    // Frontmatter for home.md now contains only the title.
    let home_frontmatter = format!("---\ntitle: \"{}\"\n---\n\n", home_title);
    let home_body = format!("{}{}", home_frontmatter, home_content);
    let mut home_file = File::create(&home_file_path)?;
    home_file.write_all(home_body.as_bytes())?;
    println!("Generated home note at {:?}", home_file_path);

    println!("Finished generating test vault in {:?}", start.elapsed());
    Ok(())
}

/// Sets up the test environment by creating:
/// - A persistent test vault at "target/test_vault/test_gnosis"
/// - A config file at "target/test_vault/.config/gnosis/config.yaml"
/// - A database file at "target/test_vault/.config/gnosis/db/database.sqlite"
/// - Sets GNOS_CONFIG_DIR to "target/test_vault/.config"
pub fn setup_test_env(note_count: usize) -> Result<(), Box<dyn Error>> {
    // Get the project root.
    let project_root = std::env::current_dir()?;
    // Define the persistent test vault directory.
    let vault_root = project_root.join("target").join("test_vault");
    let persistent_vault = vault_root.join("test_gnosis");
    if persistent_vault.exists() {
        println!("Using existing test vault at {:?}", persistent_vault);
    } else {
        println!("Test vault not found. Generating persistent test vault...");
        fs::create_dir_all(&persistent_vault)?;
        generate_test_vault(&persistent_vault, note_count)?;
        println!("Test vault generated at {:?}", persistent_vault);
    }

    // Generate image files in a subdirectory "images" inside the persistent vault.
    generate_test_images(&persistent_vault, 10)?;

    // Create a configuration directory: "target/test_vault/.config/gnosis"
    let config_dir = vault_root.join(".config").join("gnosis");
    fs::create_dir_all(&config_dir)?;

    // Write a plain YAML test configuration file.
    let persistent_vault_str = persistent_vault.to_string_lossy().to_string();
    let config_file = config_dir.join("config.yaml");
    let config_content = format!(
        r#"general:
  indicator: "test_gnosis"
vaults:
  main:
    default: true
    paths:
      - "{}"
"#,
        persistent_vault_str
    );
    fs::write(&config_file, config_content)?;
    println!("Test config written to {:?}", config_file);

    // Create a database directory: "target/test_vault/.config/gnosis/db"
    let db_dir = config_dir.join("db");
    fs::create_dir_all(&db_dir)?;
    let db_file = db_dir.join("database.sqlite");
    if !db_file.exists() {
        File::create(&db_file)?;
        println!("Created test database file at {:?}", db_file);
    }

    // Set GNOS_CONFIG_DIR to "target/test_vault/.config"
    let config_env = vault_root.join(".config");
    std::env::set_var("GNOS_CONFIG_DIR", config_env.to_str().unwrap());
    println!("GNOS_CONFIG_DIR set to {:?}", config_env);

    // Set up the database (run migrations).
    match db::setup_database() {
        Ok(_) => println!("Test database setup completed."),
        Err(e) => println!("Error setting up test database: {}", e),
    }

    Ok(())
}
