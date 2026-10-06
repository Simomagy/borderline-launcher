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

/// Log piu' recente del client TS3 (uno per avvio).
fn newest_ts3_log() -> Option<PathBuf> {
    let dir = std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(r"TS3Client\logs"))?;
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("ts3client_"))
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|e| e.path())
}

const CONFLICT_TAG: &str = "conflicting plugins loaded:";

/// Cosa dice di se' il plugin nel log del client TeamSpeak:
///   - `.0` Borderline Voice e' partito: `listening on ws://...` = si', `cannot bind` = no,
///     gli ha preso la porta un altro plugin vocale.
///   - `.1` altri plugin vocali **davvero attivi**: il plugin li vede dall'interno del processo
///     (TS3 scarica dalla memoria quelli disattivati), quindi questa lista distingue
///     "installato" da "attivo", cosa che il file su disco non puo' dire.
///
/// `None` = TS3 chiuso, log muto o plugin troppo vecchio per scriverlo: in quel caso il
/// launcher non dichiara nessun conflitto, invece di tirare a indovinare.
fn log_state() -> (Option<bool>, Option<Vec<String>>) {
    if !teamspeak_running() {
        return (None, None);
    }
    let Some(path) = newest_ts3_log() else { return (None, None) };
    let Ok(bytes) = std::fs::read(path) else { return (None, None) };
    parse_log(&String::from_utf8_lossy(&bytes))
}

/// Parte pura di `log_state`: vince sempre la riga piu' recente di ciascun tipo.
fn parse_log(text: &str) -> (Option<bool>, Option<Vec<String>>) {
    let (mut active, mut conflicts) = (None, None);
    for line in text.lines().rev() {
        if !line.contains("BorderlineVoice") {
            continue;
        }
        if conflicts.is_none() {
            if let Some(i) = line.find(CONFLICT_TAG) {
                let list = line[i + CONFLICT_TAG.len()..].trim();
                conflicts = Some(if list == "none" {
                    Vec::new()
                } else {
                    list.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                });
            }
        }
        if active.is_none() {
            if line.contains("listening on ws") {
                active = Some(true);
            } else if line.contains("cannot bind") {
                active = Some(false);
            }
        }
        if active.is_some() && conflicts.is_some() {
            break;
        }
    }
    (active, conflicts)
}

/// Stato ritornato al frontend:
/// `ok` gia' aggiornato, `installed` appena installato, `restart_needed` scaricato ma TS3
/// e' aperto, `conflict` il plugin non e' partito perche' un altro gli ha preso la porta,
/// `none` il manifest non ha `voice_plugin`, `error` con `message`.
/// Sempre presenti: `conflicts` (plugin da disattivare trovati sul disco), `plugin_active`
/// (true/false/null) e `teamspeak_running`, cosi' la guida nel launcher verifica ogni passo
/// senza altre chiamate.
#[tauri::command]
pub async fn ensure_voice_plugin() -> Value {
    let mut v = match ensure().await {
        Ok(v) => v,
        Err(e) => json!({ "status": "error", "message": e }),
    };
    let (active, reported) = log_state();
    let conflicts = reported.unwrap_or_default();
    // Blocchiamo solo su prove: il nostro plugin non e' partito, oppure un altro plugin vocale
    // risulta caricato in TeamSpeak adesso. La sola presenza della DLL su disco non conta:
    // puo' essere gia' disattivata, e segnalarla sembrerebbe un errore nostro.
    if (active == Some(false) || !conflicts.is_empty()) && v["status"] != "error" {
        v["status"] = json!("conflict");
    }
    v["conflicts"] = json!(conflicts);
    v["plugin_active"] = json!(active);
    v["teamspeak_running"] = json!(teamspeak_running());
    v
}

async fn ensure() -> Result<Value, String> {
    let dir = plugin_dir().ok_or("APPDATA non definito")?;
    let dll = dir.join(DLL_NAME);
    let staged = dir.join(format!("{DLL_NAME}.new"));
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
        return Ok(json!({ "status": "none" }));
    };
    let version = vp["version"].as_str().unwrap_or("?").to_string();
    let url = vp["url"].as_str().ok_or("manifest: voice_plugin senza url")?;
    let want = vp["sha256"].as_str().ok_or("manifest: voice_plugin senza sha256")?.to_lowercase();

    if let Ok(cur) = std::fs::read(&dll) {
        if sha256_hex(&cur) == want {
            let _ = std::fs::remove_file(&staged);
            return Ok(json!({ "status": "ok", "version": version }));
        }
    }
    if let Ok(s) = std::fs::read(&staged) {
        if sha256_hex(&s) == want {
            return Ok(json!({ "status": "restart_needed", "version": version }));
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
        return Ok(json!({ "status": "restart_needed", "version": version }));
    }
    std::fs::rename(&staged, &dll).map_err(io)?;
    Ok(json!({ "status": "installed", "version": version }))
}

#[cfg(test)]
mod tests {
    use super::parse_log;

    /// Righe vere del client TS3 (canale BorderlineVoice). Conta l'ultima di ogni tipo.
    #[test]
    fn reads_plugin_state_from_log() {
        let log = "\
2026-10-06 16:25:13.741762|INFO    |BorderlineVoice|   |listening on ws://127.0.0.1:30125, version 1.0.2
2026-10-06 16:25:18.760727|INFO    |BorderlineVoice|   |conflicting plugins loaded: SaltyChat
2026-10-06 16:25:19.000000|INFO    |ClientUI      |1  |Connect status: Connection established
";
        assert_eq!(parse_log(log), (Some(true), Some(vec!["SaltyChat".to_string()])));

        let cleared = format!("{log}2026-10-06 16:30:00.000000|INFO    |BorderlineVoice|   |conflicting plugins loaded: none\n");
        assert_eq!(parse_log(&cleared), (Some(true), Some(Vec::new())));

        let busy = "\
2026-10-06 16:25:13.000000|INFO    |BorderlineVoice|   |cannot bind 127.0.0.1:30125 (another YaCA-compatible plugin loaded?)
2026-10-06 16:25:18.000000|INFO    |BorderlineVoice|   |conflicting plugins loaded: YaCA, SaltyChat
";
        assert_eq!(
            parse_log(busy),
            (Some(false), Some(vec!["YaCA".to_string(), "SaltyChat".to_string()]))
        );

        // Plugin vecchio o TS3 appena avviato: nessuna riga nostra, nessuna pretesa.
        assert_eq!(parse_log("2026-10-06 16:25:13.000000|INFO    |Plugins       |   |Loading plugin: x.dll\n"), (None, None));
    }
}
