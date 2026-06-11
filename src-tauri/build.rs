fn main() {
    println!("cargo:rerun-if-changed=update-config.generated.rs");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let update_config = std::fs::read_to_string("update-config.generated.rs")
        .unwrap_or_else(|_| "pub const GITHUB_TOKEN: &str = \"\";\n".to_string());
    std::fs::write(out_dir.join("update_config.rs"), update_config).unwrap();

    tauri_build::build()
}
