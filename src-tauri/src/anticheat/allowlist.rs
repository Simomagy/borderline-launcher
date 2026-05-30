//! Owner-allowlist dell'Anti-Dump — fonte unica, separata dalla logica di scan.
//!
//! Owner di handle considerati legittimi (lowercase, come compaiono nel reason:
//! "... da NOME"). Overlay / registratori / AV / utility OEM aprono handle di
//! lettura ai giochi in modo legittimo. Affinare su dati reali (log "AntiDump")
//! PRIMA di attivare l'enforcement.
//!
//! È un `const` Rust: validato dal compilatore, incluso nel binario firmato e
//! NON modificabile a runtime da un attaccante.

/// Nomi eseguibili (lowercase) i cui handle su redm.exe sono leciti.
pub const OWNER_ALLOWLIST: &[&str] = &[
    "explorer.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "steamservice.exe",
    "discord.exe",
    "lenovo.modern.imcontroller.exe",
    "gameinputredistservice.exe",
    "lenovovantageservice.exe",
    "fnplicensingservice64.exe",
    "oplus_remote_service.exe",
    // Processi di sistema protetti (PPL/VSM): espongono il nome ma NON il path a
    // un processo a integrità normale, quindi il filtro su System32 non li prende
    // → allowlist per nome.
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
