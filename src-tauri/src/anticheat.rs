//! Anti-Dump watchdog (Ring 3 / user-mode).
//!
//! Un thread di background (vedi [`spawn`]) gira ogni ~5s mentre il gioco è
//! aperto e cerca processi terzi che leggono la memoria di `redm.exe` (dumper)
//! oppure finestre note di tool di dumping. Le rilevazioni vengono segnalate al
//! `pry-bridge` tramite l'heartbeat (header `status`/`reason`/`signature`).
//!
//! Modalità **report-only**: il launcher non chiude il gioco; è il server a
//! decidere (log, oppure kick se `Shared.AntiDumpEnforce`). L'heartbeat porta
//! anche la risposta a un challenge HMAC rotante che lega la connessione a un
//! binario integro.
//!
//! Tutta la scansione di basso livello è Windows-only; su altre piattaforme le
//! funzioni di scan ritornano vuoto così il progetto compila ovunque.

use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::AUTHORIZATION;
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Intervallo del ciclo di scansione + heartbeat.
const SCAN_INTERVAL: Duration = Duration::from_secs(5);

/// Owner di handle considerati legittimi (lowercase). Superficie di tuning:
/// overlay/registratori/AV aprono handle di lettura ai giochi in modo legittimo.
/// Affinare su dati reali (log "AntiDump") PRIMA di attivare l'enforcement.
const OWNER_ALLOWLIST: &[&str] = &[
    "explorer.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "steamservice.exe",
    "discord.exe",
    // Processi di sistema protetti (PPL/VSM): espongono il nome ma NON il path
    // a un processo a integrità normale, quindi il filtro su System32 non li
    // prende → allowlist per nome.
    "smss.exe",
    "lsaiso.exe",
    "csrss.exe",
    "lsass.exe",
    "services.exe",
    "svchost.exe",
    "wininit.exe",
    "winlogon.exe",
    "dwm.exe",
    "taskmgr.exe",
    "msmpeng.exe",
    "searchindexer.exe",
    "nvcontainer.exe",
    "nvsphelper64.exe",
    "nvidia web helper.exe",
    "system",
    "registry",
];

/// Classi finestra associate a tool di dumping/injection (substring, lowercase).
/// Lista iniziale, estendibile: cattura "script-kiddie" che non rinominano la
/// window class. Report-only.
const WINDOW_CLASS_BLACKLIST: &[&str] = &["scylla", "xenos", "extremeinjector", "cheatengine"];

/// Una rilevazione singola.
#[derive(Clone)]
pub struct Violation {
    pub reason: String,
    pub signature: String,
}

#[derive(Clone)]
struct Suspect {
    reason: String,
    signature: String,
}

/// Stato condiviso fra il thread di scansione e i comandi Tauri.
#[derive(Default)]
pub struct AntiCheatState {
    /// Steam hex risolto dal frontend; finché è `None` non si manda heartbeat.
    pub steam_hex: Option<String>,
    /// Violazione corrente (sticky: una volta rilevata resta per la sessione).
    pub violation: Option<Violation>,
    /// Nonce corrente del challenge rotante (aggiornato dalla risposta heartbeat).
    pub nonce: String,
    /// Esito dell'ultima verifica challenge lato server (heartbeat `trusted`).
    pub trusted: bool,
    /// Ultimo conteggio aggregato dal bridge (per la Discord Rich Presence).
    pub players: u32,
    pub launchers: u32,
}

/// Handle clonabile sullo stato (Arc) condiviso fra thread e `tauri::manage`.
#[derive(Clone, Default)]
pub struct AntiCheatHandle(pub Arc<Mutex<AntiCheatState>>);

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256(secret, nonce) in esadecimale — risposta al challenge del server.
pub fn solve_challenge(secret: &str, nonce: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accetta chiavi di qualsiasi lunghezza");
    mac.update(nonce.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// PID di tutti i processi `redm*` attivi (il gioco usa anche dei subprocess).
fn find_redm_pids(sys: &sysinfo::System) -> Vec<u32> {
    sys.processes()
        .values()
        .filter(|p| {
            p.name()
                .to_string_lossy()
                .to_lowercase()
                .starts_with("redm")
        })
        .map(|p| p.pid().as_u32())
        .collect()
}

/// Nome eseguibile (lowercase) di un PID, o "pid:N" se non risolvibile.
fn pid_name(sys: &sysinfo::System, pid: u32) -> String {
    sys.process(sysinfo::Pid::from_u32(pid))
        .map(|p| p.name().to_string_lossy().to_lowercase())
        .unwrap_or_else(|| format!("pid:{}", pid))
}

// ── Scansione handle esterni (Windows) ───────────────────────────────────────
#[cfg(windows)]
mod win {
    use super::*;
    use std::collections::HashSet;
    use windows::core::BOOL;
    use windows::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS,
    };
    use windows::Win32::Foundation::{
        CloseHandle, HWND, LPARAM, STATUS_INFO_LENGTH_MISMATCH,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_OPERATION,
        PROCESS_VM_READ, PROCESS_VM_WRITE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId,
    };

    /// SystemExtendedHandleInformation (non esposta come costante dal crate).
    const SYSTEM_EXTENDED_HANDLE_INFORMATION: SYSTEM_INFORMATION_CLASS =
        SYSTEM_INFORMATION_CLASS(64);

    /// Eseguibile in una directory di sistema Windows protetta (non scrivibile
    /// da utente non-admin). Tutti i processi OS (smss.exe, csrss.exe, lsass.exe,
    /// svchost.exe, MsMpEng.exe, …) vivono qui: filtra l'intera classe di falsi
    /// positivi di sistema in modo robusto (un finto "smss.exe" in una cartella
    /// utente NON matcha, a differenza dell'allowlist per nome).
    fn is_system_path(path: &str) -> bool {
        path.contains("\\windows\\system32\\")
            || path.contains("\\windows\\syswow64\\")
            || path.contains("\\windows\\winsxs\\")
    }

    // Le struct EX non sono definite nel crate `windows`: le replichiamo (layout x64).
    #[repr(C)]
    struct SystemHandleInformationEx {
        number_of_handles: usize,
        _reserved: usize,
        // SystemHandleTableEntryInfoEx handles[1] segue qui.
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct SystemHandleTableEntryInfoEx {
        object: *mut core::ffi::c_void,
        unique_process_id: usize,
        handle_value: usize,
        granted_access: u32,
        _creator_back_trace_index: u16,
        _object_type_index: u16,
        _handle_attributes: u32,
        _reserved: u32,
    }

    /// Snapshot globale di tutti gli handle aperti nel sistema.
    fn query_handles() -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 1024 * 1024];
        for _ in 0..16 {
            let mut return_len: u32 = 0;
            let status = unsafe {
                NtQuerySystemInformation(
                    SYSTEM_EXTENDED_HANDLE_INFORMATION,
                    buf.as_mut_ptr() as *mut core::ffi::c_void,
                    buf.len() as u32,
                    &mut return_len,
                )
            };
            if status == STATUS_INFO_LENGTH_MISMATCH {
                let needed = return_len as usize;
                let new_size = if needed > buf.len() {
                    needed + 0x20000
                } else {
                    buf.len() * 2
                };
                buf.resize(new_size, 0);
                continue;
            }
            return if status.is_ok() { Some(buf) } else { None };
        }
        None
    }

    /// Trova i processi terzi che possiedono un handle con accesso di lettura
    /// memoria verso `redm.exe`. Owner non in allowlist ⇒ sospetto.
    pub fn scan_external_handles(sys: &sysinfo::System, redm_pids: &[u32]) -> Vec<Suspect> {
        let mut suspects = Vec::new();
        if redm_pids.is_empty() {
            return suspects;
        }

        let our_pid = unsafe { GetCurrentProcessId() };
        let vm_mask = PROCESS_VM_READ.0 | PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0;

        // 1. Apri un handle a ciascun processo redm: ci serve per identificare,
        //    nello snapshot, il puntatore all'oggetto kernel del processo gioco.
        let mut my_handles = Vec::new(); // (HANDLE, handle_value as usize)
        for &pid in redm_pids {
            if let Ok(h) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
                my_handles.push((h, h.0 as usize));
            }
        }
        if my_handles.is_empty() {
            return suspects;
        }

        // 2. Snapshot DOPO l'apertura (così i nostri handle compaiono).
        let buf = match query_handles() {
            Some(b) => b,
            None => {
                for (h, _) in &my_handles {
                    let _ = unsafe { CloseHandle(*h) };
                }
                return suspects;
            }
        };

        let base = buf.as_ptr();
        let header =
            unsafe { std::ptr::read_unaligned(base as *const SystemHandleInformationEx) };
        let count = header.number_of_handles;
        let entries =
            unsafe { base.add(std::mem::size_of::<SystemHandleInformationEx>()) }
                as *const SystemHandleTableEntryInfoEx;

        let my_handle_values: HashSet<usize> = my_handles.iter().map(|(_, v)| *v).collect();

        // 3. Object pointer dei processi redm (le nostre voci nello snapshot).
        let mut redm_objects: HashSet<usize> = HashSet::new();
        for i in 0..count {
            let e = unsafe { std::ptr::read_unaligned(entries.add(i)) };
            if e.unique_process_id as u32 == our_pid && my_handle_values.contains(&e.handle_value) {
                redm_objects.insert(e.object as usize);
            }
        }

        // 4. Chi altro punta a quegli oggetti con accesso di lettura memoria?
        let redm_set: HashSet<u32> = redm_pids.iter().copied().collect();
        let mut reported: HashSet<String> = HashSet::new();
        if !redm_objects.is_empty() {
            for i in 0..count {
                let e = unsafe { std::ptr::read_unaligned(entries.add(i)) };
                let owner = e.unique_process_id as u32;
                if owner == our_pid || owner == 0 || owner == 4 || redm_set.contains(&owner) {
                    continue;
                }
                if !redm_objects.contains(&(e.object as usize)) {
                    continue;
                }
                if e.granted_access & vm_mask == 0 {
                    continue;
                }
                let proc = sys.process(sysinfo::Pid::from_u32(owner));
                let name = proc
                    .map(|p| p.name().to_string_lossy().to_lowercase())
                    .unwrap_or_else(|| format!("pid:{}", owner));
                let path = proc
                    .and_then(|p| p.exe())
                    .map(|p| p.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                // Salta gli owner legittimi:
                //  - path in System32/SysWOW64/WinSxS (processi OS normali);
                //  - path NON leggibile (vuoto) → processo protetto PPL/VSM o a
                //    privilegio superiore al launcher (smss, lsaiso, fontdrvhost,
                //    audiodg, …): un dumper user-mode gira alla stessa integrità
                //    dell'utente e il suo path è leggibile, quindi questo elimina
                //    l'intera classe di falsi positivi di sistema. (Trade-off: un
                //    dumper lanciato come admin avrebbe il path non leggibile da un
                //    launcher non elevato → eseguire il launcher elevato per coprirlo.)
                //  - owner esplicitamente in allowlist.
                if is_system_path(&path)
                    || path.is_empty()
                    || OWNER_ALLOWLIST.contains(&name.as_str())
                    || reported.contains(&name)
                {
                    continue;
                }
                reported.insert(name.clone());
                suspects.push(Suspect {
                    reason: format!("Handle di lettura memoria su redm.exe da {}", name),
                    signature: format!("ExternalHandle:{}", name),
                });
            }
        }

        for (h, _) in &my_handles {
            let _ = unsafe { CloseHandle(*h) };
        }
        suspects
    }

    /// Callback EnumWindows: raccoglie (class_name, owner_pid) per finestre con
    /// classe in blacklist.
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let hits = unsafe { &mut *(lparam.0 as *mut Vec<(String, u32)>) };
        let mut buf = [0u16; 256];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        if len > 0 {
            let class = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
            if WINDOW_CLASS_BLACKLIST.iter().any(|c| class.contains(c)) {
                let mut pid = 0u32;
                unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
                hits.push((class, pid));
            }
        }
        BOOL(1) // continua l'enumerazione
    }

    /// Cerca finestre con class name note di tool di dumping.
    pub fn scan_window_classes(sys: &sysinfo::System) -> Vec<Suspect> {
        let mut hits: Vec<(String, u32)> = Vec::new();
        let _ = unsafe {
            EnumWindows(
                Some(enum_proc),
                LPARAM(&mut hits as *mut Vec<(String, u32)> as isize),
            )
        };
        hits.into_iter()
            .map(|(class, pid)| {
                let owner = if pid != 0 {
                    pid_name(sys, pid)
                } else {
                    "sconosciuto".to_string()
                };
                Suspect {
                    reason: format!("Window class di dumping '{}' ({})", class, owner),
                    signature: format!("WindowClass:{}", class),
                }
            })
            .collect()
    }
}

#[cfg(not(windows))]
fn scan_external_handles(_sys: &sysinfo::System, _redm_pids: &[u32]) -> Vec<Suspect> {
    Vec::new()
}
#[cfg(not(windows))]
fn scan_window_classes(_sys: &sysinfo::System) -> Vec<Suspect> {
    Vec::new()
}

/// Invia l'heartbeat al bridge (chiamato dal thread di background, reqwest blocking).
fn send_heartbeat(client: &reqwest::blocking::Client, handle: &AntiCheatHandle) {
    let (steam, status, reason, signature, nonce) = {
        let st = handle.0.lock().unwrap();
        let steam = match &st.steam_hex {
            Some(s) => s.clone(),
            None => return, // identità non ancora nota → niente heartbeat
        };
        let (status, reason, signature) = match &st.violation {
            Some(v) => ("violation", v.reason.clone(), v.signature.clone()),
            None => ("ok", String::new(), String::new()),
        };
        (steam, status, reason, signature, st.nonce.clone())
    };

    let auth = solve_challenge(crate::LAUNCHER_HMAC_SECRET, &nonce);
    let url = format!("{}/api/v1/launcher-heartbeat", crate::BRIDGE_URL);

    let resp = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", crate::BRIDGE_API_KEY))
        .header("steam", &steam)
        .header("status", status)
        .header("reason", &reason)
        .header("signature", &signature)
        .header("auth", &auth)
        .send();

    if let Ok(r) = resp {
        if let Ok(text) = r.text() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                let mut st = handle.0.lock().unwrap();
                if let Some(p) = v.get("players").and_then(|x| x.as_u64()) {
                    st.players = p as u32;
                }
                if let Some(l) = v.get("launchers").and_then(|x| x.as_u64()) {
                    st.launchers = l as u32;
                }
                if let Some(c) = v.get("challenge").and_then(|x| x.as_str()) {
                    st.nonce = c.to_string();
                }
                if let Some(t) = v.get("trusted").and_then(|x| x.as_bool()) {
                    st.trusted = t;
                }
            }
        }
    }
}

/// Avvia il thread di background: scan + heartbeat ogni [`SCAN_INTERVAL`].
pub fn spawn(handle: AntiCheatHandle) {
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .build()
            .ok();

        loop {
            let mut sys = sysinfo::System::new();
            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, false);
            let redm_pids = find_redm_pids(&sys);

            if !redm_pids.is_empty() {
                let mut suspects = Vec::new();
                #[cfg(windows)]
                {
                    suspects.extend(win::scan_external_handles(&sys, &redm_pids));
                    suspects.extend(win::scan_window_classes(&sys));
                }
                #[cfg(not(windows))]
                {
                    suspects.extend(scan_external_handles(&sys, &redm_pids));
                    suspects.extend(scan_window_classes(&sys));
                }

                if let Some(s) = suspects.into_iter().next() {
                    let mut st = handle.0.lock().unwrap();
                    // Sticky: non sovrascrivere una violazione già registrata.
                    if st.violation.is_none() {
                        st.violation = Some(Violation {
                            reason: s.reason,
                            signature: s.signature,
                        });
                    }
                }
            }

            if let Some(client) = &client {
                send_heartbeat(client, &handle);
            }

            // Finché non c'è un nonce stabilito (identità appena impostata, primo
            // heartbeat non ancora andato a buon fine) batti veloce, così il
            // challenge è pronto prima che l'utente possa premere GIOCA. Una volta
            // stabilito, torna alla cadenza normale.
            let established = {
                let st = handle.0.lock().unwrap();
                st.steam_hex.is_some() && !st.nonce.is_empty()
            };
            std::thread::sleep(if established {
                SCAN_INTERVAL
            } else {
                Duration::from_secs(1)
            });
        }
    });
}
