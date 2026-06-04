//! Allowlist anti-dump **remota e firmata**, scaricata dal CDN.
//!
//! La lista effettiva usata dallo scan è l'unione `BASE ∪ remota`:
//!   - `BASE_*` (in [`super::allowlist`]) è il "pavimento" compilato nel binario:
//!     copre i processi di sistema e garantisce un comportamento sensato anche
//!     offline / al primo avvio;
//!   - la parte remota può solo **aggiungere** nomi (allenta il rilevamento),
//!     mai toglierne. Così un fetch fallito o assente lascia l'anti-cheat *più
//!     severo* (direzione sicura), e l'unico attacco pericoloso — falsificare la
//!     lista per nascondere un dumper — è bloccato dalla firma **minisign**.
//!
//! Flusso (vedi `ALLOWLIST-CDN-SPEC.md`): GET condizionale (ETag) su un client
//! TLS-validante; su `200` si verifica la firma sui byte esatti del JSON, si
//! controlla l'anti-rollback su `version`, si fa il merge e si salva una copia
//! last-known-good su disco (riverificata al load).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

use super::allowlist::{BASE_OWNER_ALLOWLIST, BASE_WINDOW_CLASS_BLACKLIST};

/// Chiave pubblica minisign — è la riga-chiave di `updater.pubkey` in
/// `tauri.conf.json` (lì è codificata in base64 col commento; qui solo la riga
/// `RW…`). Stessa keypair che firma le release: aggiornare entrambe se ruota.
const ALLOWLIST_PUBKEY_B64: &str = "RWSRwjggmVtFOX9sOQYyMPbLza0iZPwK2N7prT16nWNgVJYSWHnEcvnC";

const ALLOWLIST_URL: &str = "https://cdn.borderlinerp.com/anticheat/latest.json";
const ALLOWLIST_SIG_URL: &str = "https://cdn.borderlinerp.com/anticheat/latest.json.minisig";

/// Documento JSON servito dal CDN.
#[derive(Deserialize)]
struct AllowlistDoc {
    /// Intero monotòno crescente (anti-rollback).
    version: u64,
    #[serde(default)]
    owner_allow: Vec<String>,
    #[serde(default)]
    window_class_block: Vec<String>,
}

/// Liste effettive (BASE ∪ remota) condivise con lo scan.
struct EffectiveLists {
    owner_allow: HashSet<String>,
    window_block: Vec<String>,
    /// Versione remota applicata (0 = solo BASE).
    version: u64,
    /// ETag dell'ultima `latest.json` applicata (per la GET condizionale).
    etag: Option<String>,
}

impl EffectiveLists {
    /// Stato iniziale: solo la BASE compilata.
    fn base_only() -> Self {
        EffectiveLists {
            owner_allow: BASE_OWNER_ALLOWLIST.iter().map(|s| s.to_string()).collect(),
            window_block: BASE_WINDOW_CLASS_BLACKLIST
                .iter()
                .map(|s| s.to_string())
                .collect(),
            version: 0,
            etag: None,
        }
    }
}

static LISTS: LazyLock<RwLock<EffectiveLists>> =
    LazyLock::new(|| RwLock::new(EffectiveLists::base_only()));

// ── Accessori usati dallo scan ───────────────────────────────────────────────

/// Owner (nome eseguibile lowercase) è in allowlist?
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_owner_allowed(name: &str) -> bool {
    LISTS
        .read()
        .map(|l| l.owner_allow.contains(name))
        .unwrap_or(false)
}

/// La window class (lowercase) contiene una substring in blacklist?
#[cfg_attr(not(windows), allow(dead_code))]
pub fn window_block_matches(class: &str) -> bool {
    LISTS
        .read()
        .map(|l| l.window_block.iter().any(|c| class.contains(c.as_str())))
        .unwrap_or(false)
}

// ── Fetch / verifica / applicazione ──────────────────────────────────────────

/// Decodifica base64 standard, trimming incluso.
fn b64_decode(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .ok()
}

/// Interpreta il contenuto del file firma. Supporta due formati:
///   - **Tauri** (`tauri signer sign`): `base64( testo-firma-minisign )` → un
///     decode e poi il blocco minisign standard;
///   - **minisign puro**: già il testo `untrusted comment: …`.
fn parse_minisig(sig_text: &str) -> Option<Signature> {
    let trimmed = sig_text.trim();
    if let Some(decoded) = b64_decode(trimmed) {
        if let Ok(s) = String::from_utf8(decoded) {
            if s.contains("untrusted comment") {
                if let Ok(sig) = Signature::decode(&s) {
                    return Some(sig);
                }
            }
        }
    }
    Signature::decode(trimmed).ok()
}

/// Verifica la firma sui byte esatti del JSON, applica l'anti-rollback, fa il
/// merge `BASE ∪ remota` e (se `write_cache`) salva la copia su disco.
/// Ritorna `true` solo se la lista è stata applicata.
fn apply_verified(body: &[u8], sig_text: &str, etag: Option<String>, write_cache: bool) -> bool {
    let Some(sig) = parse_minisig(sig_text) else {
        return false;
    };
    let Ok(pk) = PublicKey::from_base64(ALLOWLIST_PUBKEY_B64) else {
        return false;
    };
    // `false` = niente firme legacy: pretendiamo il formato prehashed di Tauri.
    if pk.verify(body, &sig, false).is_err() {
        return false;
    }
    let Ok(doc) = serde_json::from_slice::<AllowlistDoc>(body) else {
        return false;
    };

    // Anti-rollback: mai applicare una versione più vecchia di quella corrente
    // (impedisce di rifilare una lista vecchia validamente firmata).
    if doc.version < LISTS.read().map(|l| l.version).unwrap_or(0) {
        return false;
    }

    // Merge: la BASE è sempre inclusa; la remota può solo aggiungere.
    let mut owner: HashSet<String> = BASE_OWNER_ALLOWLIST.iter().map(|s| s.to_string()).collect();
    owner.extend(doc.owner_allow.iter().map(|s| s.to_lowercase()));
    let mut window: Vec<String> = BASE_WINDOW_CLASS_BLACKLIST
        .iter()
        .map(|s| s.to_string())
        .collect();
    for c in &doc.window_class_block {
        let lc = c.to_lowercase();
        if !window.contains(&lc) {
            window.push(lc);
        }
    }

    if let Ok(mut w) = LISTS.write() {
        w.owner_allow = owner;
        w.window_block = window;
        w.version = doc.version;
        w.etag = etag.clone();
    }

    if write_cache {
        save_cache(body, sig_text, etag.as_deref());
    }
    true
}

/// GET condizionale dal CDN; su aggiornamento verifica e applica. Best-effort:
/// qualsiasi errore di rete/verifica lascia intatta la lista corrente.
pub fn refresh(client: &reqwest::blocking::Client) {
    let etag = LISTS.read().ok().and_then(|l| l.etag.clone());

    let mut req = client.get(ALLOWLIST_URL);
    if let Some(tag) = &etag {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(tag) {
            req = req.header(reqwest::header::IF_NONE_MATCH, v);
        }
    }

    let Ok(resp) = req.send() else {
        return;
    };
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED || !resp.status().is_success() {
        return;
    }
    let new_etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let Ok(body) = resp.bytes() else {
        return;
    };
    let Ok(sig_text) = client.get(ALLOWLIST_SIG_URL).send().and_then(|r| r.text()) else {
        return;
    };

    apply_verified(body.as_ref(), &sig_text, new_etag, true);
}

// ── Cache su disco (last-known-good) ─────────────────────────────────────────

/// `%APPDATA%\com.borderlinerp.launcher\anticheat\` (identifier di tauri.conf.json).
fn cache_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let mut p = PathBuf::from(appdata);
    p.push("com.borderlinerp.launcher");
    p.push("anticheat");
    Some(p)
}

fn save_cache(body: &[u8], sig_text: &str, etag: Option<&str>) {
    let Some(dir) = cache_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("allowlist.json"), body);
    let _ = std::fs::write(dir.join("allowlist.json.minisig"), sig_text);
    if let Some(tag) = etag {
        let _ = std::fs::write(dir.join("allowlist.etag"), tag);
    }
}

/// Carica all'avvio l'ultima lista valida dal disco e **ne riverifica la firma**
/// prima di usarla, così un CDN irraggiungibile non azzera il tuning già fatto.
pub fn load_cache() {
    let Some(dir) = cache_dir() else {
        return;
    };
    let Ok(body) = std::fs::read(dir.join("allowlist.json")) else {
        return;
    };
    let Ok(sig_text) = std::fs::read_to_string(dir.join("allowlist.json.minisig")) else {
        return;
    };
    let etag = std::fs::read_to_string(dir.join("allowlist.etag"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // `write_cache = false`: stiamo proprio leggendo dalla cache.
    apply_verified(&body, &sig_text, etag, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifica end-to-end che una firma prodotta da `tauri signer sign` (formato
    /// Tauri = base64 della firma minisign) sia accettata dal path crittografico
    /// del client contro la pubkey di `tauri.conf.json`. Richiede gli artefatti
    /// firmati localmente: `pwsh ./publish-allowlist.ps1 -DryRun`.
    #[test]
    #[ignore = "richiede anticheat/allowlist.json(.sig) firmati localmente"]
    fn accepts_real_tauri_signature() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../anticheat/");
        let body = std::fs::read(format!("{dir}allowlist.json")).expect("allowlist.json");
        let sig = std::fs::read_to_string(format!("{dir}allowlist.json.sig")).expect("allowlist.json.sig");
        assert!(
            apply_verified(&body, &sig, None, false),
            "firma Tauri reale NON verificata dal client (pubkey o unwrap base64?)"
        );

        // Controprova: un solo byte cambiato deve invalidare la firma.
        let mut tampered = body.clone();
        tampered[0] ^= 0xFF;
        assert!(
            !apply_verified(&tampered, &sig, None, false),
            "un payload manomesso NON deve passare la verifica"
        );
    }

    /// Integrazione: fetch reale dal CDN live, verifica firma e applica. Richiede
    /// rete e che una versione firmata sia pubblicata. Esegui:
    /// `cargo test fetches_and_applies_from_live_cdn -- --ignored`.
    #[test]
    #[ignore = "rete: colpisce il CDN di produzione"]
    fn fetches_and_applies_from_live_cdn() {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent(crate::USER_AGENT)
            .build()
            .unwrap();
        refresh(&client);
        let v = LISTS.read().unwrap().version;
        assert!(v >= 1, "il CDN non ha applicato nessuna versione (v={v})");
        eprintln!("CDN live: applicata allowlist version {v}");
    }
}
