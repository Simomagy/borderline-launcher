use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use reqwest::header::{AUTHORIZATION, HeaderValue};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

mod heartbeat;
use heartbeat::HeartbeatHandle;

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

// User-Agent HTTP riconoscibile: un client senza UA (default `reqwest/x.y`) è
// uno dei tratti che insospettisce l'euristica degli antivirus.
pub(crate) const USER_AGENT: &str = concat!("BorderlineLauncher/", env!("CARGO_PKG_VERSION"));

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
        // Se l'URI ha uno schema e questo non risulta dichiarato da nessuna
        // parte, fermati subito con un errore utile invece di delegare a una
        // shell che fallira' in silenzio davanti al giocatore. Vale per redm://,
        // discord://, steam:// e ts3server:// allo stesso modo.
        if let Some(scheme) = uri_scheme(uri)
            && !scheme_declared(scheme)
        {
            return Err(format!(
                "Nessuna applicazione registrata per {scheme}:// — installazione mancante o danneggiata"
            ));
        }

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

/// Avvia un eseguibile con un argomento, staccato dal job object del launcher.
/// Serve quando NON si può passare dalla shell: `explorer.exe` accetta un solo
/// item e non inoltra argomenti, quindi per passare un URL a un exe specifico
/// bisogna spawnarlo direttamente.
#[cfg(target_os = "windows")]
fn spawn_detached_exe(exe: &std::path::Path, arg: &str) -> Result<(), String> {
    let launch = |flags: u32| {
        std::process::Command::new(exe)
            .arg(arg)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    // Con breakaway dal job object; se il job non lo consente, riprova senza.
    if launch(BREAKAWAY_FLAGS | NO_WINDOW).is_ok() {
        return Ok(());
    }
    launch(NO_WINDOW).map(|_| ()).map_err(|e| e.to_string())
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

/// Host del server vocale TeamSpeak di Borderline (porta default 9987, canale
/// "Lobby" come primo canale senza password). Single source of truth, come gli
/// altri target di connessione (redm/discord/steam).
const TEAMSPEAK_HOST: &str = "ts3dev.borderlinerp.com";

/// Cerca l'eseguibile di TeamSpeak nel registro (uninstall entries con
/// DisplayName "TeamSpeak*"). È la fonte autorevole: copre installazioni
/// per-utente (%LOCALAPPDATA%\Programs) e cartelle non standard, che i path
/// fissi mancano. `DisplayIcon` punta già all'exe (es. "...\ts3client_win64.exe,0");
/// in fallback uso `InstallLocation` + nome eseguibile noto.
#[cfg(target_os = "windows")]
fn teamspeak_from_registry() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::RegKey;

    let uninstall_roots = [
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];

    for (root, base) in uninstall_roots {
        let Ok(uninstall) = RegKey::predef(root).open_subkey(base) else { continue };
        for sub in uninstall.enum_keys().flatten() {
            let Ok(entry) = uninstall.open_subkey(&sub) else { continue };
            let name: String = entry.get_value("DisplayName").unwrap_or_default();
            if !name.contains("TeamSpeak") {
                continue;
            }
            // DisplayIcon: "C:\...\ts3client_win64.exe,0" → togli l'indice icona e gli apici.
            if let Ok(icon) = entry.get_value::<String, _>("DisplayIcon") {
                let exe = icon.split(',').next().unwrap_or("").trim().trim_matches('"');
                let p = PathBuf::from(exe);
                if p.is_file() {
                    return Some(p);
                }
            }
            if let Ok(loc) = entry.get_value::<String, _>("InstallLocation") {
                for exe in ["TeamSpeak.exe", "ts3client_win64.exe", "ts3client_win32.exe"] {
                    let p = PathBuf::from(&loc).join(exe);
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// Schema di un URI (`redm://connect/...` -> `redm`), se ne ha uno.
#[cfg(target_os = "windows")]
fn uri_scheme(uri: &str) -> Option<&str> {
    let (scheme, _) = uri.split_once("://")?;
    let ok = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    ok.then_some(scheme)
}

/// Path dell'eseguibile in una riga di comando di un handler. Gestisce sia
/// `"C:\...\app.exe" "%1"` sia `C:\...\app.exe %1`, e i path con spazi non
/// quotati (taglia sull'ultima estensione `.exe`).
#[cfg(target_os = "windows")]
fn command_exe(cmd: &str) -> Option<String> {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string).filter(|s| !s.is_empty());
    }
    if let Some(i) = cmd.to_lowercase().rfind(".exe") {
        return Some(cmd[..i + 4].to_string());
    }
    cmd.split_whitespace().next().map(str::to_string).filter(|s| !s.is_empty())
}

/// Espande i `%VAR%` (i comandi degli handler sono spesso REG_EXPAND_SZ, es.
/// `%ProgramFiles%\App\app.exe`). Le variabili non risolte restano invariate.
#[cfg(target_os = "windows")]
fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(j) = after.find('%') else {
            out.push('%');
            rest = after;
            break;
        };
        let name = &after[..j];
        match std::env::var(name) {
            Ok(v) => out.push_str(&v),
            Err(_) => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    out
}

/// Classi (ProgId) che possono gestire uno schema URL, nell'ordine in cui
/// Windows le consulta. Sono TRE meccanismi diversi e un'app puo' usarne uno
/// qualsiasi:
///  1. `UserChoice` - l'associazione scelta esplicitamente dall'utente;
///  2. lo schema stesso - forma classica `<scheme>\shell\open\command`
///     (Steam, Discord, TeamSpeak);
///  3. `RegisteredApplications` -> `Capabilities\URLAssociations` - forma usata
///     da RedM: la chiave `redm` dichiara solo `URL Protocol` e il comando vive
///     nella ProgId `RedM.ProtocolHandler`. Guardare solo il punto 2 qui fa
///     concludere "non registrato" per un handler che funziona benissimo.
#[cfg(target_os = "windows")]
fn scheme_classes(scheme: &str) -> Vec<String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let mut out = Vec::new();

    let user_choice = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Shell\Associations\UrlAssociations\{scheme}\UserChoice"
    );
    if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&user_choice)
        && let Ok(progid) = k.get_value::<String, _>("ProgId")
    {
        out.push(progid);
    }

    out.push(scheme.to_string());

    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let Ok(apps) = RegKey::predef(root).open_subkey(r"Software\RegisteredApplications") else {
            continue;
        };
        for name in apps.enum_values().flatten().map(|(n, _)| n) {
            let Ok(caps_path) = apps.get_value::<String, _>(&name) else { continue };
            let Ok(caps) = RegKey::predef(root).open_subkey(format!(r"{caps_path}\URLAssociations"))
            else {
                continue;
            };
            if let Ok(progid) = caps.get_value::<String, _>(scheme) {
                out.push(progid);
            }
        }
    }

    out
}

/// La classe esiste nel registro (classi dell'utente o vista unita HKCR)?
#[cfg(target_os = "windows")]
fn class_exists(class: &str) -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(format!(r"Software\Classes\{class}"))
        .is_ok()
        || RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(class).is_ok()
}

/// `Some(true)` = la classe ha un `shell\open\command` utilizzabile;
/// `Some(false)` = ce l'ha ma punta a un file che non esiste; `None` = nessun
/// comando (classe assente o senza `shell\open\command`).
///
/// Fail-safe: comando non interpretabile, eseguibile senza directory (risolto
/// via PATH) o handler di app pacchettizzata (`DelegateExecute`) contano come
/// utilizzabili - meglio provare che rifiutare un avvio che funzionerebbe.
#[cfg(target_os = "windows")]
fn class_command_usable(class: &str) -> Option<bool> {
    use winreg::enums::*;
    use winreg::RegKey;

    let tail = format!(r"{class}\shell\open\command");
    let roots = [
        (HKEY_CURRENT_USER, format!(r"Software\Classes\{tail}")),
        (HKEY_CLASSES_ROOT, tail),
    ];

    for (root, key) in roots {
        let Ok(k) = RegKey::predef(root).open_subkey(&key) else { continue };
        if k.get_value::<String, _>("DelegateExecute").is_ok() {
            return Some(true);
        }
        let Ok(cmd) = k.get_value::<String, _>("") else { continue };
        let Some(exe) = command_exe(&cmd) else { return Some(true) };
        let path = expand_env(&exe);
        let path = std::path::Path::new(&path);
        if path.is_file() || path.parent().is_none_or(|d| d.as_os_str().is_empty()) {
            return Some(true);
        }
        return Some(false);
    }
    None
}

/// Lo schema e' dichiarato da qualche parte in Windows? Se non lo e', nessuna
/// shell potra' mai dispatcharlo: e' l'UNICO caso in cui rifiutiamo un avvio.
/// Volutamente permissivo - il costo di un falso negativo (gioco che non parte)
/// e' molto piu' alto di quello di un falso positivo (un errore di shell).
#[cfg(target_os = "windows")]
fn scheme_declared(scheme: &str) -> bool {
    scheme_classes(scheme).iter().any(|c| class_exists(c))
}

/// Lo schema si risolve a un eseguibile che esiste davvero? Usato solo dove c'e'
/// un'alternativa migliore da scegliere (TeamSpeak), mai per bloccare un avvio.
#[cfg(target_os = "windows")]
fn scheme_resolves_to_exe(scheme: &str) -> bool {
    scheme_classes(scheme)
        .iter()
        .any(|c| class_command_usable(c) == Some(true))
}

/// Risolve l'eseguibile di TeamSpeak: prima dal registro (qualsiasi cartella
/// d'installazione), poi dai path fissi noti (TS5 + TS3 64/32-bit), sia
/// per-utente (%LOCALAPPDATA%\Programs) sia per-macchina (%ProgramFiles%).
#[cfg(target_os = "windows")]
fn teamspeak_exe() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    if let Some(path) = teamspeak_from_registry() {
        return Some(path);
    }

    [
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join(r"Programs\TeamSpeak 5\TeamSpeak.exe")),
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join(r"Programs\TeamSpeak 3 Client\ts3client_win64.exe")),
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join(r"Programs\TeamSpeak 3 Client\ts3client_win32.exe")),
        std::env::var_os("ProgramFiles")
            .map(|p| PathBuf::from(p).join(r"TeamSpeak 3 Client\ts3client_win64.exe")),
        std::env::var_os("ProgramFiles(x86)")
            .map(|p| PathBuf::from(p).join(r"TeamSpeak 3 Client\ts3client_win32.exe")),
    ]
    .into_iter()
    .flatten()
    .find(|p| p.exists())
}

/// Avvia TeamSpeak e si connette direttamente al server vocale di Borderline.
///
/// Usa lo schema URL `ts3server://`, registrato dall'installer di TeamSpeak
/// (handler: `ts3client "%1"`): è lo schema *stabile* per connettersi a un
/// server — l'equivalente di `redm://connect/...`. Viene delegato a explorer.exe
/// come gli altri launcher, così TeamSpeak resta figlio della shell e non del
/// launcher (niente kill a catena alla chiusura). Dalla 1.3.5 il launcher gira
/// `asInvoker`, quindi anche il fallback diretto qui sotto avvia TeamSpeak alla
/// stessa integrity di RedM, come richiede il plugin vocale (YACA).
///
/// IMPORTANTE: niente query string. `explorer.exe` non sa fare il dispatch di un
/// URL con `?...` e ripiegherebbe aprendo la cartella Documenti (bug osservato
/// con `?nickname=`). Porta (9987) e canale ("Lobby") sono già i default del
/// server; il nickname lo imposta YACA in-game, quindi non va precompilato qui.
#[tauri::command]
fn launch_teamspeak() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let url = format!("ts3server://{TEAMSPEAK_HOST}");

        // Percorso primario: handler di protocollo via shell, coerente con
        // redm/discord/steam — ma solo se lo schema si risolve davvero a un
        // eseguibile presente. Senza questo controllo un `ts3server://` rotto
        // farebbe comunque ritornare Ok (explorer fa il dispatch in modo
        // asincrono) e il fallback qui sotto non verrebbe mai raggiunto.
        if scheme_resolves_to_exe("ts3server") && spawn_detached(&url).is_ok() {
            return Ok(());
        }

        // Handler assente o rotto: avvia l'eseguibile risolto passando l'URL come
        // argomento. È esattamente ciò che farebbe l'handler (`ts3client "%1"`),
        // quindi la connessione automatica al server funziona lo stesso.
        if let Some(path) = teamspeak_exe() {
            return spawn_detached_exe(&path, &url);
        }

        // Ultima spiaggia: prova comunque la shell.
        spawn_detached(&url)
    }

    #[cfg(not(target_os = "windows"))]
    Err("Non supportato su questa piattaforma".to_string())
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
        .user_agent(USER_AGENT)
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
        .user_agent(USER_AGENT)
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
    state: tauri::State<'_, HeartbeatHandle>,
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
    let auth = heartbeat::solve_challenge(LAUNCHER_HMAC_SECRET, &nonce);

    let url = format!("{}/api/v1/authorize-entry", BRIDGE_URL);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent(USER_AGENT)
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
        .user_agent(USER_AGENT)
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
    state: tauri::State<'_, HeartbeatHandle>,
    steam_hex: String,
) -> Result<(), String> {
    state.0.lock().map_err(|e| e.to_string())?.steam_hex = Some(steam_hex);
    Ok(())
}

/// Conteggio aggregato dall'ultimo heartbeat del thread (per la Rich Presence).
#[tauri::command]
fn get_heartbeat_counts(state: tauri::State<'_, HeartbeatHandle>) -> serde_json::Value {
    let st = state.0.lock().unwrap();
    serde_json::json!({ "players": st.players, "launchers": st.launchers })
}

/// Stato del canale d'integrità: usato dal frontend per abilitare GIOCA.
/// `authenticated` = challenge verificato dal server E heartbeat già stabilito.
/// Il launcher non ispeziona più i processi terzi (vedi `heartbeat.rs`), quindi
/// non esiste più uno stato di "violazione" lato client.
#[tauri::command]
fn get_heartbeat_status(state: tauri::State<'_, HeartbeatHandle>) -> serde_json::Value {
    let st = state.0.lock().unwrap();
    let authenticated = st.trusted && !st.nonce.is_empty() && st.steam_hex.is_some();
    serde_json::json!({
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

/// Riporta la finestra principale davanti da QUALSIASI stato: nascosta nel
/// tray (`hide()`), minimizzata nella taskbar, o semplicemente coperta da altre
/// finestre. L'ordine conta: `set_focus()` su una finestra nascosta non fa
/// nulla, e `show()` da solo non ripristina una finestra minimizzata.
fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let heartbeat = HeartbeatHandle::default();
    let heartbeat_thread = heartbeat.clone();

    tauri::Builder::default()
        // DEVE restare il PRIMO plugin registrato: il suo hook di setup gira
        // prima di tutti gli altri, quindi una seconda istanza viene terminata
        // prima di costruire finestre, tray o webview.
        //
        // Senza questo lock si aprivano N launcher sullo stesso PC, con due
        // conseguenze: N volte le chiamate all'API, e soprattutto autenticazione
        // rotta — il nonce del challenge ruota a ogni heartbeat, quindi due
        // istanze con lo stesso Steam hex si rubano il nonce a vicenda e l'HMAC
        // dell'altra diventa stale (`trusted` = false).
        //
        // Il callback gira nell'istanza GIA' attiva quando l'utente rilancia
        // l'exe: al posto di una finestra nuova, riporta davanti quella che c'e'.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            focus_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .manage(DiscordPresence::default())
        .manage(heartbeat)
        .setup(move |app| {
            // Avviato qui e non prima del `Builder`: una seconda istanza esce
            // durante la costruzione e non deve aver fatto partire un thread di
            // heartbeat concorrente nel frattempo.
            heartbeat::spawn(heartbeat_thread);

            let show = MenuItem::with_id(app, "show", "Mostra Launcher", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("BorderlineRP Launcher")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => focus_main_window(app),
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
                        focus_main_window(tray.app_handle());
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
            launch_teamspeak,
            get_steam_hex,
            get_steam_profile,
            check_player_access,
            authorize_entry,
            set_launcher_identity,
            get_heartbeat_counts,
            get_heartbeat_status,
            update_discord_presence,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    /// Verifica che il client di produzione (rustls, SENZA bypass della
    /// validazione) completi l'handshake TLS verso `BRIDGE_URL` e riceva 2xx.
    /// rustls e' piu' severo di curl/openssl: un certificato che passa da riga
    /// di comando puo' comunque essere rifiutato qui. Da rilanciare ogni volta
    /// che `BRIDGE_URL` cambia o che si tocca il reverse proxy.
    ///
    /// `cargo test bridge_tls -- --ignored --nocapture`
    #[test]
    #[ignore = "rete: colpisce il bridge di produzione"]
    fn bridge_tls() {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent(USER_AGENT)
            .build()
            .expect("costruzione client");

        let resp = client
            .get(format!("{BRIDGE_URL}/api/v1/health"))
            .header(AUTHORIZATION, format!("Bearer {BRIDGE_API_KEY}"))
            .send()
            .expect("handshake TLS / rete fallita");

        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        eprintln!("{BRIDGE_URL}/api/v1/health -> {status}");
        eprintln!("  {}", &body[..body.len().min(140)]);
        assert!(status.is_success(), "atteso 2xx, ricevuto {status}");
        assert!(
            BRIDGE_URL.starts_with("https://"),
            "BRIDGE_URL non e' https: il traffico viaggia ancora in chiaro"
        );
    }

    /// Diagnostica: stampa come Windows risolve gli schemi che il launcher usa
    /// su QUESTA macchina. Dipende dalle app installate, quindi non asserisce
    /// nulla di universale — serve a capire cosa vede il launcher quando un
    /// giocatore riporta "Nessuna applicazione registrata per X://".
    ///
    /// `cargo test scheme_resolution -- --ignored --nocapture`
    #[test]
    #[ignore = "dipende dalle app installate sulla macchina"]
    fn scheme_resolution() {
        for scheme in ["redm", "discord", "steam", "ts3server"] {
            let classi = scheme_classes(scheme);
            eprintln!(
                "{scheme:<10} dichiarato={:<5} exe_ok={:<5} classi={:?}",
                scheme_declared(scheme),
                scheme_resolves_to_exe(scheme),
                classi,
            );
            for c in &classi {
                eprintln!("             {c} -> esiste={:?} comando={:?}", class_exists(c), class_command_usable(c));
            }
        }
    }
}
