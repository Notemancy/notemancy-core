use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("meilisearch");

    let asset_file = if target.contains("windows") {
        "meilisearch-windows-amd64.exe"
    } else if target.contains("apple") {
        if target.contains("aarch64") {
            "meilisearch-macos-apple-silicon"
        } else {
            "meilisearch-macos-amd64"
        }
    } else if target.contains("linux") {
        if target.contains("aarch64") {
            "meilisearch-linux-aarch64"
        } else {
            "meilisearch-linux-amd64"
        }
    } else {
        panic!("Unsupported target: {}", target);
    };

    let asset_path = Path::new(&manifest_dir).join("assets").join(asset_file);
    fs::copy(&asset_path, &dest_path).unwrap_or_else(|_| {
        panic!(
            "Failed to copy MeiliSearch binary from {:?} to {:?}",
            asset_path, dest_path
        )
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest_path, perms).unwrap();
    }

    println!("cargo:rerun-if-changed=build.rs");
}
