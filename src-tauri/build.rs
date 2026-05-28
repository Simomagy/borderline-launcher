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
    println!("cargo:rerun-if-changed=app-dev.manifest");

    // Manifest Windows condizionale al profilo:
    //  - release → requireAdministrator (AC vede i path dei processi elevati;
    //    l'utente finale vedrà il prompt UAC ad ogni avvio del launcher).
    //  - debug   → asInvoker (così `tauri dev` parte da una shell normale senza
    //    UAC; in dev l'AC non vede i path dei processi elevati — accettabile).
    let is_release = std::env::var("PROFILE").as_deref() == Ok("release");
    let manifest = if is_release {
        include_str!("app.manifest")
    } else {
        include_str!("app-dev.manifest")
    };
    let attrs = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(manifest),
    );
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
