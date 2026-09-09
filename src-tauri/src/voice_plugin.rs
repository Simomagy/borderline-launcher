//! Aggiornamento automatico del plugin vocale TS3 (Borderline Voice).
//!
//! Il manifest dell'updater (`latest.json`) porta una chiave extra `voice_plugin`
//! `{ version, url, sha256 }` che Tauri ignora. Qui la usiamo per tenere allineata
//! `%APPDATA%\TS3Client\plugins\borderline_voice_win64.dll` sul PC del giocatore:
//! TS3 carica tutte le DLL di quella cartella all'avvio, senza registrazione.
//!
//! Vincolo: una DLL caricata da TS3 non si puo' sovrascrivere. Con TS3 chiuso si
//! installa subito (write su `.new` + rename, mai una DLL a meta'); con TS3 aperto
//! si lascia il `.new` in attesa e lo si applica al prossimo avvio del launcher,
//! o alla prossima chiamata, appena TS3 e' chiuso.
//! Pubblicazione lato dev: `release/publish-voice-plugin.ps1` (cartella ignorata da git).
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

const MANIFEST_URL: &str = "https://cdn.borderlinerp.com/manifest/latest.json";
const DLL_NAME: &str = "borderline_voice_win64.dll";

fn plugin_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(r"TS3Client\plugins"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn teamspeak_running() -> bool {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, false);
    sys.processes().values().any(|p| {
        matches!(
            p.name().to_string_lossy().to_lowercase().as_str(),
            "ts3client_win64.exe" | "ts3client_win32.exe"
        )
    })
}

/// Plugin TS3 che usano la stessa porta 30125 o lo stesso protocollo: con uno di questi
/// installato Borderline Voice non parte. Il launcher guida il giocatore a disinstallarli.
const CONFLICTING: [(&str, &str); 2] = [("yaca_win64.dll", "YaCA"), ("SaltyChat_win64.dll", "SaltyChat")];

/// Stato ritornato al frontend:
/// `ok` gia' aggiornato, `installed` appena installato, `restart_needed` scaricato ma TS3
/// e' aperto, `conflict` c'e' un plugin incompatibile (lista in `conflicts`),
/// `none` il manifest non ha `voice_plugin`, `error` con `message`.
/// Sempre presenti: `conflicts` e `teamspeak_running`, cosi' la guida nel launcher
/// puo' verificare ogni passo senza altre chiamate.
#[tauri::command]
pub async fn ensure_voice_plugin() -> Value {
    let dir = plugin_dir();
    let conflicts: Vec<&str> = dir
        .as_ref()
        .map(|d| CONFLICTING.iter().filter(|(f, _)| d.join(f).exists()).map(|(_, n)| *n).collect())
        .unwrap_or_default();
    let mut v = match ensure().await {
        Ok(v) => v,
        Err(e) => json!({ "status": "error", "message": e }),
    };
    if !conflicts.is_empty() && v["status"] != "error" {
        v["status"] = json!("conflict");
    }
    v["conflicts"] = json!(conflicts);
    v["teamspeak_running"] = json!(teamspeak_running());
    v
}

async fn ensure() -> Result<Value, String> {
    let dir = plugin_dir().ok_or("APPDATA non definito")?;
    let dll = dir.join(DLL_NAME);
    let staged = dir.join(format!("{DLL_NAME}.new"));
    let yaca_present = dir.join("yaca_win64.dll").exists();
    let io = |e: std::io::Error| e.to_string();

    // Update rimasto in attesa da una volta precedente: applicalo se TS3 e' chiuso.
    if staged.exists() && !teamspeak_running() {
        std::fs::rename(&staged, &dll).map_err(io)?;
    }

    let client = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let manifest: Value = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let Some(vp) = manifest.get("voice_plugin") else {
        return Ok(json!({ "status": "none", "yaca_present": yaca_present }));
    };
    let version = vp["version"].as_str().unwrap_or("?").to_string();
    let url = vp["url"].as_str().ok_or("manifest: voice_plugin senza url")?;
    let want = vp["sha256"].as_str().ok_or("manifest: voice_plugin senza sha256")?.to_lowercase();

    if let Ok(cur) = std::fs::read(&dll) {
        if sha256_hex(&cur) == want {
            let _ = std::fs::remove_file(&staged);
            return Ok(json!({ "status": "ok", "version": version, "yaca_present": yaca_present }));
        }
    }
    if let Ok(s) = std::fs::read(&staged) {
        if sha256_hex(&s) == want {
            return Ok(json!({ "status": "restart_needed", "version": version, "yaca_present": yaca_present }));
        }
    }

    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    if sha256_hex(&bytes) != want {
        return Err("sha256 del plugin scaricato non corrisponde al manifest".into());
    }

    std::fs::create_dir_all(&dir).map_err(io)?;
    std::fs::write(&staged, &bytes).map_err(io)?;
    if teamspeak_running() {
        return Ok(json!({ "status": "restart_needed", "version": version, "yaca_present": yaca_present }));
    }
    std::fs::rename(&staged, &dll).map_err(io)?;
    Ok(json!({ "status": "installed", "version": version, "yaca_present": yaca_present }))
}
