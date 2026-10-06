//! Pulizia della cache di RedM, su richiesta esplicita del giocatore.
//!
//! Due modalita':
//!   - `full`   cancella `cache`, `nui-storage`, `server-cache`, `server-cache-priv`
//!              (RedM le ricrea al prossimo avvio; il primo avvio dopo e' piu' lento)
//!   - `vulkan` cancella solo i file `hints_*` dentro `cache`, il fix per l'errore
//!              Vulkan all'avvio: non tocca gli snapshot dell'eseguibile.
//!
//! Non tocca nulla fuori da `%LOCALAPPDATA%\RedM\RedM.app\data` e rifiuta se RedM e' aperto.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Sottocartelle cancellate dalla modalita' `full`. Nomi fissi: niente input dal frontend nei path.
const FULL_DIRS: [&str; 4] = ["cache", "nui-storage", "server-cache", "server-cache-priv"];

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join(r"RedM\RedM.app\data"))
}

fn redm_running() -> bool {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, false);
    sys.processes()
        .values()
        .any(|p| p.name().to_string_lossy().to_lowercase() == "redm.exe")
}

/// Byte occupati da un file o da una cartella (ricorsiva). Gli errori valgono 0: e' solo
/// il numero da mostrare a fine pulizia, non deve far fallire la cancellazione.
fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0; // symlink/junction: non ci entriamo
    }
    std::fs::read_dir(path)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| size_of(&e.path())).sum())
        .unwrap_or(0)
}

/// `mode`: "full" o "vulkan". Ritorna `{ status, removed, freed_mb, failed }`,
/// con `status` = `ok` | `redm_running` | `error`.
#[tauri::command]
pub fn clear_redm_cache(mode: String) -> Value {
    let Some(data) = data_dir() else {
        return json!({ "status": "error", "message": "LOCALAPPDATA non definito" });
    };
    if !data.exists() {
        return json!({ "status": "error", "message": format!("cartella RedM non trovata: {}", data.display()) });
    }
    if redm_running() {
        return json!({ "status": "redm_running" });
    }

    // Lista dei bersagli: sempre dentro `data`, sempre da nomi nostri.
    let targets: Vec<PathBuf> = match mode.as_str() {
        "full" => FULL_DIRS.iter().map(|d| data.join(d)).collect(),
        "vulkan" => std::fs::read_dir(data.join("cache"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("hints_"))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default(),
        other => return json!({ "status": "error", "message": format!("modalita' sconosciuta: {other}") }),
    };

    let mut removed = Vec::new();
    let mut failed = Vec::new();
    let mut freed: u64 = 0;
    for t in targets {
        if !t.exists() {
            continue;
        }
        let size = size_of(&t);
        let res = if t.is_dir() { std::fs::remove_dir_all(&t) } else { std::fs::remove_file(&t) };
        let name = t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match res {
            Ok(()) => {
                freed += size;
                removed.push(name);
            }
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }

    json!({
        "status": if failed.is_empty() { "ok" } else { "error" },
        "removed": removed,
        "freed_mb": (freed as f64 / 1_048_576.0 * 10.0).round() / 10.0,
        "failed": failed,
        "message": failed.join("; "),
    })
}
