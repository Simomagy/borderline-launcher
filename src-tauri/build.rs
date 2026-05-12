fn main() {
    // Legge src-tauri/.env e inietta le variabili come costanti Rust a compile-time.
    // Il file .env è gitignored — le credenziali non entrano mai nel repository.
    if let Ok(content) = std::fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = line.split_once('=') {
                println!("cargo:rustc-env={}={}", key.trim(), val.trim());
            }
        }
    }
    // Ri-esegui build.rs se .env cambia
    println!("cargo:rerun-if-changed=.env");

    tauri_build::build()
}
