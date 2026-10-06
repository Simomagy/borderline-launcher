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

/// Plugin vocali incompatibili col nostro: chiave `info=` dell'addon in TeamSpeak e nome
/// da mostrare al giocatore. Vanno **disattivati**, mai disinstallati.
const CONFLICTING: [(&str, &str); 2] = [("yaca", "YaCA"), ("SaltyChat", "SaltyChat")];

/// Plugin vocali davvero **abilitati**, letti dal database di TeamSpeak (tabella `Addons`).
///
/// E' l'unica fonte attendibile. Non lo dice il file su disco (puo' essere li' e disattivato) e
/// nemmeno la lista dei moduli caricati nel processo: TeamSpeak carica la DLL di ogni plugin
/// anche quando e' disabilitato (verificato il 6/10/2026: SaltyChat con `enabled=false` risultava
/// comunque fra i moduli di ts3client). Nelle righe della tabella, un addon abilitato
/// semplicemente non ha la chiave `enabled`.
///
/// `None` = database illeggibile: nessuna pretesa, il launcher non dichiara conflitti.
fn enabled_conflicts() -> Option<Vec<String>> {
    let db = std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(r"TS3Client\settings.db"))?;
    // Copia: TeamSpeak tiene il file aperto, noi leggiamo uno scatto senza disturbarlo.
    let snapshot = std::env::temp_dir().join("borderline_ts3_settings.db");
    std::fs::copy(&db, &snapshot).ok()?;
    let conn = rusqlite::Connection::open_with_flags(&snapshot, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut stmt = conn.prepare("SELECT value FROM Addons").ok()?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .ok()?
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect();
    Some(conflicts_in_rows(&rows))
}

/// Parte pura di `enabled_conflicts`.
fn conflicts_in_rows(rows: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows {
        if !row.lines().any(|l| l.trim() == "type=PLUGIN") || row.lines().any(|l| l.trim() == "enabled=false") {
            continue;
        }
        for (info, name) in CONFLICTING {
            if row.lines().any(|l| l.trim() == format!("info={info}")) && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Borderline Voice e' partito in TeamSpeak? Lo dice il plugin stesso nel log del client:
/// `listening on ws://...` = si', `cannot bind` = un altro plugin vocale gli ha preso la porta.
/// `None` = TS3 chiuso o log muto.
fn plugin_active() -> Option<bool> {
    if !teamspeak_running() {
        return None;
    }
    let bytes = std::fs::read(newest_ts3_log()?).ok()?;
    parse_log(&String::from_utf8_lossy(&bytes))
}

/// Parte pura di `plugin_active`: vince la riga piu' recente.
fn parse_log(text: &str) -> Option<bool> {
    for line in text.lines().rev() {
        if !line.contains("BorderlineVoice") {
            continue;
        }
        if line.contains("listening on ws") {
            return Some(true);
        }
        if line.contains("cannot bind") {
            return Some(false);
        }
    }
    None
}

/// Stato ritornato al frontend:
/// `ok` gia' aggiornato, `installed` appena installato, `restart_needed` scaricato ma TS3
/// e' aperto, `conflict` il plugin non e' partito perche' un altro gli ha preso la porta,
/// `none` il manifest non ha `voice_plugin`, `error` con `message`.
/// Sempre presenti: `conflicts` (plugin vocali abilitati in TeamSpeak), `plugin_active`
/// (true/false/null) e `teamspeak_running`, cosi' la guida nel launcher verifica ogni passo
/// senza altre chiamate.
#[tauri::command]
pub async fn ensure_voice_plugin() -> Value {
    let mut v = match ensure().await {
        Ok(v) => v,
        Err(e) => json!({ "status": "error", "message": e }),
    };
    let active = plugin_active();
    let conflicts = enabled_conflicts().unwrap_or_default();
    // Blocchiamo solo su prove: il nostro plugin non e' partito, oppure TeamSpeak ha un altro
    // plugin vocale abilitato. La DLL ferma su disco, o caricata ma disattivata, non conta:
    // segnalarla sembrerebbe un errore nostro.
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
    use super::{conflicts_in_rows, parse_log};

    /// Righe vere del client TS3 (canale BorderlineVoice): conta l'ultima.
    #[test]
    fn reads_plugin_state_from_log() {
        let ok = "2026-10-06 16:25:13.741762|INFO    |BorderlineVoice|   |listening on ws://127.0.0.1:30125, version 1.0.3
";
        assert_eq!(parse_log(ok), Some(true));

        let busy = format!("{ok}2026-10-06 16:30:00.000000|INFO    |BorderlineVoice|   |cannot bind 127.0.0.1:30125 (another YaCA-compatible plugin loaded?)
");
        assert_eq!(parse_log(&busy), Some(false));

        assert_eq!(parse_log("2026-10-06 16:25:13.000000|INFO    |Plugins       |   |Loading plugin: x.dll
"), None);
    }

    /// Controllo manuale sulla macchina corrente: legge davvero settings.db di TeamSpeak.
    /// `cargo test live_addons -- --ignored --nocapture`
    #[test]
    #[ignore = "dipende dal TeamSpeak installato su questa macchina"]
    fn live_addons() {
        println!("plugin vocali abilitati: {:?}", super::enabled_conflicts());
        assert!(super::enabled_conflicts().is_some(), "settings.db illeggibile");
    }

    /// Righe vere della tabella Addons di settings.db: un addon abilitato non ha la chiave `enabled`.
    #[test]
    fn reads_enabled_plugins_from_addons_rows() {
        let salty_off = "type=PLUGIN
author=saltyhub.net
version=4.1.0
info=SaltyChat
uninstall_capable=true
name=Salty Chat
enabled=false";
        let salty_on = salty_off.replace("
enabled=false", "");
        let yaca_off = "uninstall_capable=true
enabled=false
type=PLUGIN
name=Yet-Another-Communication-Addon
info=yaca";
        let soundboard = "uninstall_capable=true
type=PLUGIN
info=rp_soundboard
name=RP Soundboard";
        let iconpack = "uninstall_capable=true
type=ICONPACK
info=SaltyChat
name=finto";

        let rows = [salty_off.to_string(), yaca_off.to_string(), soundboard.to_string(), iconpack.to_string()];
        assert_eq!(conflicts_in_rows(&rows), Vec::<String>::new());

        let rows = [salty_on.clone(), yaca_off.to_string(), soundboard.to_string()];
        assert_eq!(conflicts_in_rows(&rows), vec!["SaltyChat".to_string()]);

        let rows = [salty_on, yaca_off.replace("
enabled=false", "")];
        assert_eq!(conflicts_in_rows(&rows), vec!["SaltyChat".to_string(), "YaCA".to_string()]);
    }
}
