//! Presenza del launcher e challenge d'integrità (Ring 3 / user-mode).
//!
//! Un thread di background (vedi [`spawn`]) manda un heartbeat al `pry-bridge`
//! ogni [`HEARTBEAT_INTERVAL`]. L'heartbeat fa tre cose:
//!   - tiene viva la **presenza**: se smette di arrivare, il bridge kicka il
//!     giocatore rimasto in gioco senza launcher (fail-closed);
//!   - risponde al **challenge HMAC rotante**, che lega la sessione a un binario
//!     integro (un client rifatto non ottiene poi `authorize_entry`);
//!   - riporta i conteggi aggregati (`players`/`launchers`) per la Rich Presence.
//!
//! **Scansione anti-dump rimossa (1.3.4).** Il launcher non ispeziona più i
//! processi di terze parti. Il segnale su cui si basava — un handle su
//! `redm.exe` con `PROCESS_VM_READ` — è richiesto da overlay, driver di
//! periferiche, antivirus, componenti della shell e bloatware OEM: l'allowlist
//! era il complemento di un insieme illimitato (era arrivata a 407 nomi, v59)
//! e ogni falso positivo bloccava GIOCA a un giocatore legittimo. In cambio non
//! copriva i cheat reali di RedM, che girano dentro il processo del gioco (DLL
//! iniettate, executor Lua) o fuori dalla portata dello user-mode (driver
//! kernel, DMA). Il rilevamento vive lato server, dove i falsi positivi da
//! software desktop non esistono per costruzione.

use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::AUTHORIZATION;
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Cadenza dell'heartbeat verso il bridge: tiene viva la presenza e il
/// binding-kick. DEVE restare ben sotto `LauncherHeartbeatTimeout`/`PresenceTTL`
/// lato bridge, altrimenti i giocatori in-game vengono espulsi.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Stato condiviso fra il thread di heartbeat e i comandi Tauri.
#[derive(Default)]
pub struct AntiCheatState {
    /// Steam hex risolto dal frontend; finché è `None` non si manda heartbeat.
    pub steam_hex: Option<String>,
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

/// Invia l'heartbeat al bridge (chiamato dal thread di background, reqwest blocking).
/// Gli header `status`/`reason`/`signature` restano nel protocollo per
/// compatibilità con il bridge, ma il launcher non ha più nulla da segnalare.
fn send_heartbeat(client: &reqwest::blocking::Client, handle: &AntiCheatHandle) {
    let (steam, nonce) = {
        let st = handle.0.lock().unwrap();
        let steam = match &st.steam_hex {
            Some(s) => s.clone(),
            None => return, // identità non ancora nota → niente heartbeat
        };
        (steam, st.nonce.clone())
    };

    let auth = solve_challenge(crate::LAUNCHER_HMAC_SECRET, &nonce);
    let url = format!("{}/api/v1/launcher-heartbeat", crate::BRIDGE_URL);

    let resp = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", crate::BRIDGE_API_KEY))
        .header("steam", &steam)
        .header("status", "ok")
        .header("reason", "")
        .header("signature", "")
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

/// Avvia il thread di background: heartbeat ogni [`HEARTBEAT_INTERVAL`].
pub fn spawn(handle: AntiCheatHandle) {
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .user_agent(crate::USER_AGENT)
            .build()
            .ok();

        loop {
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
                HEARTBEAT_INTERVAL
            } else {
                Duration::from_secs(1)
            });
        }
    });
}
