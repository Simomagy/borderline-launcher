//! Liste BASE compilate nel binario: il "pavimento" sempre attivo, unito a
//! runtime con la lista remota firmata (vedi `remote.rs`). NON aggiungere qui i
//! nomi di tuning quotidiano: vanno in `anticheat/allowlist.json` e pubblicati
//! sul CDN. Questa lista è il fallback offline e va tenuta MINIMA: copre solo i
//! lettori legittimi che NON sono filtrabili per path e che devono restare
//! leciti già al primo avvio / con CDN irraggiungibile (prima che la prima
//! `refresh()` arrivi). Per tutto il resto c'è il CDN.

/// Nomi eseguibili (lowercase) i cui handle su redm.exe sono leciti. Una entry
/// che termina con `*` è un **prefisso** (es. `codesetup-*` allowlista ogni
/// processo il cui nome inizia con `codesetup-`); stessa sintassi vale nella
/// lista remota dal CDN. Vedi `split_allow` in `remote.rs`.
///
/// Tenuta minima di proposito: i processi in `\Windows\System32`, `SysWOW64`,
/// `WinSxS` sono già filtrati per path, e quelli protetti/ad alta integrità
/// hanno path non leggibile (filtro `path.is_empty()` nello scan). Restano da
/// coprire qui solo i lettori con path leggibile FUORI da System32 (overlay del
/// launcher/gioco) e — per difesa in profondità — i protetti notori.
pub const BASE_OWNER_ALLOWLIST: &[&str] = &[
    // ── Ecosistema launcher/gioco ────────────────────────────────────────────
    // Path leggibile fuori da System32 → NON coperti dai filtri su path. Sono i
    // lettori legittimi più comuni (overlay) e devono restare leciti offline.
    "borderline-launcher.exe",
    "launcher.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "steamservice.exe",
    "gameoverlayui64.exe",
    "rockstarservice.exe",
    "socialclubhelper.exe",
    "rockstarerrorhandler.exe",
    // ── Processi di sistema protetti (PPL/VSM) ───────────────────────────────
    // Espongono il nome ma spesso non il path a un processo a integrità normale.
    // Il filtro `path.is_empty()`/System32 di solito li copre già; elencati qui
    // per nome come difesa in profondità se il path risultasse parzialmente
    // risolvibile.
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
    "system",
    "registry",
];

/// Classi finestra (substring, lowercase) associate a tool di dumping/injection.
/// Lista iniziale, estendibile via CDN (`window_class_block` in allowlist.json).
pub const BASE_WINDOW_CLASS_BLACKLIST: &[&str] =
    &["scylla", "xenos", "extremeinjector", "cheatengine"];
