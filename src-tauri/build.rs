fn main() {
    println!("cargo:rerun-if-env-changed=HARNESSLITE_EDITION");
    let edition = std::env::var("HARNESSLITE_EDITION").unwrap_or_else(|_| "lite".into());
    println!("cargo:rustc-env=HARNESSLITE_EDITION={edition}");
    tauri_build::build();
}
