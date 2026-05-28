use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use reqwest::header::{AUTHORIZATION, HeaderValue};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

mod anticheat;
use anticheat::AntiCheatHandle;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const BREAKAWAY_FLAGS: u32 = 0x01000000 | 0x00000008 | 0x00000200;
const NO_WINDOW: u32 = 0x08000000;

pub(crate) const BRIDGE_URL: &str = env!("BRIDGE_URL");
pub(crate) const BRIDGE_API_KEY: &str = env!("BRIDGE_API_KEY");
const STEAM_WEBAPI_KEY: &str = env!("STEAM_WEBAPI_KEY");
const DISCORD_CLIENT_ID: &str = env!("DISCORD_CLIENT_ID");
// Segreto condiviso col bridge per il challenge/response HMAC (anti-tamper).
pub(crate) const LAUNCHER_HMAC_SECRET: &str = env!("LAUNCHER_HMAC_SECRET");

// ── Discord Rich Presence — configurazione ──────────────────────────────────
// Gli asset (key 'logo') vanno caricati nel Developer Portal Discord:
//   Rich Presence → Art Assets. %players è sostituito a runtime con
//   (giocatori in-game + launcher aperti).
const RPC_STATE_TEMPLATE: &str = "In gioco — %players giocatori";
const RPC_LARGE_IMAGE: &str = "logo";
const RPC_LARGE_TEXT: &str = "Borderline";
const RPC_BUTTONS: [(&str, &str); 2] = [
    ("Sito Web", "https://borderlinerp.com"),
    ("Server Discord", "https://ds.borderlinerp.com"),
];

/// Client Discord IPC condiviso fra le invocazioni. `None` finché non connesso.
#[derive(Default)]
struct DiscordPresence(Mutex<Option<DiscordIpcClient>>);

/// Scansiona una volta il processo table e ritorna lo stato di Discord, Steam e RedM.
/// Una sola chiamata API invece di tre spawn di tasklist.
#[tauri::command]
fn check_processes() -> serde_json::Value {
    use sysinfo::{ProcessesToUpdate, System};

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, false);

    let mut discord   = false;
    let mut steam     = false;
    let mut redm      = false;
    let mut teamspeak = false;

    for process in sys.processes().values() {
        match process.name().to_string_lossy().to_lowercase().as_str() {
            "discord.exe" => discord = true,
            "steam.exe"   => steam   = true,
            "redm.exe"    => redm    = true,
            // TeamSpeak 3 (ts3client_win64/32) e TeamSpeak 5 (teamspeak.exe) — backend vocale YACA
            "ts3client_win64.exe" | "ts3client_win32.exe" | "teamspeak.exe" => teamspeak = true,
            _ => {}
        }
        if discord && steam && redm && teamspeak { break; }
    }

    serde_json::json!({ "discord": discord, "steam": steam, "redm": redm, "teamspeak": teamspeak })
}

fn spawn_detached(uri: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Metodo 1: explorer.exe delega all'istanza shell già in esecuzione (fuori dal job object).
        // Discord/Steam/RedM diventano figli di explorer, non del launcher → non vengono killati.
        if std::process::Command::new("explorer")
            .arg(uri)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            return Ok(());
        }

        // Metodo 2: cmd /C start con breakaway dal job object.
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", "", uri])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(BREAKAWAY_FLAGS | NO_WINDOW);

        if cmd.spawn().is_ok() {
            return Ok(());
        }

        // Metodo 3: cmd /C start senza flag (fallback — funziona in build finale, non in dev).
        std::process::Command::new("cmd")
            .args(["/C", "start", "", uri])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    std::process::Command::new("open")
        .arg(uri)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn launch_discord() -> Result<(), String> {
    spawn_detached("discord://")
}

#[tauri::command]
fn launch_steam() -> Result<(), String> {
    spawn_detached("steam://open/main")
}

#[tauri::command]
fn launch_game() -> Result<(), String> {
    spawn_detached("redm://connect/rmabxvx")
}

/// Legge l'utente Steam attivo dal registro Windows e ritorna hex + SteamID64.
/// HKCU\Software\Valve\Steam\ActiveProcess\ActiveUser = SteamID3 (DWORD).
/// SteamID64 = 76561197960265728 + SteamID3.
#[tauri::command]
fn get_steam_hex() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey("Software\\Valve\\Steam\\ActiveProcess")
            .map_err(|_| "Registro Steam non trovato. Assicurati che Steam sia installato.".to_string())?;

        let active_user: u32 = key
            .get_value("ActiveUser")
            .map_err(|_| "ActiveUser non trovato nel registro Steam.".to_string())?;

        if active_user == 0 {
            return Err("Steam non è connesso. Accedi a Steam e riprova.".to_string());
        }

        let id64: u64 = 76561197960265728u64 + active_user as u64;
        Ok(serde_json::json!({
            "hex": format!("{:x}", id64),
            "id64": id64.to_string(),
        }))
    }

    #[cfg(not(target_os = "windows"))]
    Err("Lettura del registro non supportata su questa piattaforma.".to_string())
}

/// Recupera nome e avatar di un profilo Steam tramite Steam Web API.
#[tauri::command]
async fn get_steam_profile(id64: String) -> Result<String, String> {
    let url = format!(
        "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/?key={}&steamids={}",
        STEAM_WEBAPI_KEY, id64
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// Verifica accesso al server per un dato steam hex via pry-bridge.
/// Il bridge accetta l'hex senza prefisso "steam:" (lo aggiunge internamente).
#[tauri::command]
async fn check_player_access(steam_hex: String) -> Result<String, String> {
    let url = format!("{}/api/v1/isUserAllowlisted", BRIDGE_URL);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    client
        .get(&url)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", BRIDGE_API_KEY))
                .map_err(|e| e.to_string())?,
        )
        .header("steam", &steam_hex)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// POST /api/v1/authorize-entry — comunica al bridge che questo steam hex sta per connettersi.
/// Allega la risposta al challenge corrente (HMAC del nonce gestito dal thread
/// anti-cheat): un client modificato che non ha la catena heartbeat valida non
/// ottiene l'autorizzazione e quindi non può connettersi.
#[tauri::command]
async fn authorize_entry(
    state: tauri::State<'_, AntiCheatHandle>,
    steam_hex: String,
) -> Result<String, String> {
    // Attendi un nonce valido (il thread heartbeat lo popola ad ogni risposta).
    // Copre la corsa "GIOCA premuto prima del primo heartbeat".
    let mut nonce = String::new();
    for _ in 0..30 {
        nonce = state.0.lock().map_err(|e| e.to_string())?.nonce.clone();
        if !nonce.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let auth = anticheat::solve_challenge(LAUNCHER_HMAC_SECRET, &nonce);

    let url = format!("{}/api/v1/authorize-entry", BRIDGE_URL);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    client
        .post(&url)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", BRIDGE_API_KEY))
                .map_err(|e| e.to_string())?,
        )
        .header("steam", &steam_hex)
        .header("auth", &auth)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// GET autenticata a pry-bridge. Il frontend passa solo l'endpoint (es. "/api/v1/health").
#[tauri::command]
async fn fetch_bridge(endpoint: String) -> Result<String, String> {
    let url = format!("{}{}", BRIDGE_URL, endpoint);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    client
        .get(&url)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", BRIDGE_API_KEY))
                .map_err(|e| e.to_string())?,
        )
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// Comunica al thread anti-cheat lo steam hex risolto dal frontend; da quel
/// momento il thread inizia a mandare l'heartbeat (con scan + challenge).
#[tauri::command]
fn set_launcher_identity(
    state: tauri::State<'_, AntiCheatHandle>,
    steam_hex: String,
) -> Result<(), String> {
    state.0.lock().map_err(|e| e.to_string())?.steam_hex = Some(steam_hex);
    Ok(())
}

/// Conteggio aggregato dall'ultimo heartbeat del thread (per la Rich Presence).
#[tauri::command]
fn get_heartbeat_counts(state: tauri::State<'_, AntiCheatHandle>) -> serde_json::Value {
    let st = state.0.lock().unwrap();
    serde_json::json!({ "players": st.players, "launchers": st.launchers })
}

/// Stato anti-cheat corrente: usato dal frontend per abilitare/bloccare GIOCA.
/// `authenticated` = challenge verificato dal server E heartbeat già stabilito;
/// `violation` = dumper rilevato. GIOCA va abilitato solo se authenticated && !violation.
#[tauri::command]
fn get_anticheat_status(state: tauri::State<'_, AntiCheatHandle>) -> serde_json::Value {
    let st = state.0.lock().unwrap();
    let authenticated = st.trusted && !st.nonce.is_empty() && st.steam_hex.is_some();
    let (violation, reason, signature) = match &st.violation {
        Some(v) => (true, v.reason.clone(), v.signature.clone()),
        None => (false, String::new(), String::new()),
    };
    serde_json::json!({
        "violation": violation,
        "reason": reason,
        "signature": signature,
        "trusted": st.trusted,
        "authenticated": authenticated,
    })
}

/// Aggiorna (o inizializza alla prima chiamata) la Rich Presence di Discord.
/// `players` è il totale già calcolato dal frontend (in-game + launcher aperti).
#[tauri::command]
fn update_discord_presence(
    presence: tauri::State<'_, DiscordPresence>,
    players: u32,
) -> Result<(), String> {
    let mut guard = presence.0.lock().map_err(|e| e.to_string())?;

    // Connessione lazy: la prima chiamata apre l'IPC verso il client Discord.
    if guard.is_none() {
        let mut client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
        client.connect().map_err(|e| e.to_string())?;
        *guard = Some(client);
    }
    let client = guard.as_mut().unwrap();

    let state_text = RPC_STATE_TEMPLATE.replace("%players", &players.to_string());
    let activity = activity::Activity::new()
        .state(&state_text)
        .assets(
            activity::Assets::new()
                .large_image(RPC_LARGE_IMAGE)
                .large_text(RPC_LARGE_TEXT),
        )
        .buttons(
            RPC_BUTTONS
                .iter()
                .map(|(label, url)| activity::Button::new(*label, *url))
                .collect(),
        );

    // Se Discord è stato chiuso l'IPC fallisce: azzera lo stato così il
    // prossimo update tenta una nuova connessione.
    if let Err(e) = client.set_activity(activity) {
        *guard = None;
        return Err(e.to_string());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let anticheat = AntiCheatHandle::default();
    anticheat::spawn(anticheat.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .manage(DiscordPresence::default())
        .manage(anticheat)
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Mostra Launcher", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("BorderlineRP Launcher")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                })
                .build(app)?;

            let win = app.get_webview_window("main").unwrap();
            let win_hide = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = win_hide.hide();
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_processes,
            fetch_bridge,
            launch_game,
            launch_discord,
            launch_steam,
            get_steam_hex,
            get_steam_profile,
            check_player_access,
            authorize_entry,
            set_launcher_identity,
            get_heartbeat_counts,
            get_anticheat_status,
            update_discord_presence,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
