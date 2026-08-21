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
    // Ri-esegui build.rs se .env o i manifest cambiano
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=app.manifest");

    // Manifest Windows unico (asInvoker) per dev e release: dalla rimozione
    // della scansione anti-dump (1.3.4) il launcher non ha più alcun motivo di
    // girare elevato, quindi niente prompt UAC ad ogni avvio e niente
    // disallineamento di integrity con i processi che avvia.
    let attrs = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    );
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
