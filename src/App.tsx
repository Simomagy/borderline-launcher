import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

interface HealthData {
  success: boolean;
  data?: { status: string; uptime: string; system: { players: number; memory_usage: string } };
}
interface SteamProfile { name: string; avatar: string; }
interface BanInfo {
  reason?: string; admin?: string; date?: string;
  ban_type?: "temporary" | "permanent"; expires_at?: string;
}
type ServerStatus  = "loading" | "online" | "offline";
type AccessStatus  = "loading" | "allowed" | "not_allowlisted" | "banned" | "error" | "unknown";

const win = () => getCurrentWindow();

const WESTERN_MESSAGES = [
  "Contando i fagioli", "Oliando i revolver", "Sellando il cavallo",
  "Lucidando gli speroni", "Cercando lo sceriffo", "Affilando il machete",
  "Imbottigliando il whisky", "Tracciando la mappa", "Pulendo il fucile",
  "Domando il puledro", "Radunando il bestiame", "Accendendo il falò",
];

const KEYFRAMES = `
  @keyframes float        { 0%,100%{transform:translateY(0)} 50%{transform:translateY(-8px)} }
  @keyframes glow-pulse   {
    0%,100%{ box-shadow: 0 0 28px rgba(201,161,74,.22), 0 0 0 1px rgba(201,161,74,.18); }
    50%    { box-shadow: 0 0 80px rgba(201,161,74,.58), 0 0 160px rgba(201,161,74,.10), 0 0 0 1px rgba(201,161,74,.42); }
  }
  @keyframes shimmer-x    { 0%{transform:translateX(-100%)} 100%{transform:translateX(400%)} }
  @keyframes fade-up      { from{opacity:0;transform:translateY(14px)} to{opacity:1;transform:translateY(0)} }
  @keyframes fadeIn       { from{opacity:0;transform:translateY(4px)}  to{opacity:1;transform:translateY(0)} }
  @keyframes card-in      { from{opacity:0;transform:translateY(-6px)} to{opacity:1;transform:translateY(0)} }
  @keyframes emporio-glow {
    0%,100%{ box-shadow: 0 0 0 1px rgba(201,161,74,.25); }
    50%    { box-shadow: 0 0 18px rgba(201,161,74,.28), 0 0 0 1px rgba(201,161,74,.50); }
  }
`;

// ── primitivi ─────────────────────────────────────────────────────────────────

function Diamond({ cls = "" }: { cls?: string }) {
  return <span className={`block w-1.5 h-1.5 rotate-45 flex-shrink-0 ${cls}`} />;
}

function Pip({ status }: { status: ServerStatus }) {
  if (status === "online")
    return (
      <span className="relative flex h-1.5 w-1.5 flex-shrink-0">
        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-moss-500 opacity-60" />
        <span className="relative inline-flex rounded-full h-1.5 w-1.5 bg-moss-500" />
      </span>
    );
  if (status === "offline") return <span className="h-1.5 w-1.5 rounded-full bg-blood-600 flex-shrink-0" />;
  return <span className="h-1.5 w-1.5 rounded-full bg-white/20 animate-pulse flex-shrink-0" />;
}

// ── loading center ─────────────────────────────────────────────────────────────

function LoadingCenter({ label }: { label: string }) {
  const [idx, setIdx] = useState(0);
  const [dots, setDots] = useState(".");

  useEffect(() => {
    const t = setInterval(() => setIdx(i => (i + 1) % WESTERN_MESSAGES.length), 2500);
    return () => clearInterval(t);
  }, []);
  useEffect(() => {
    const t = setInterval(() => setDots(d => d.length >= 3 ? "." : d + "."), 400);
    return () => clearInterval(t);
  }, []);

  return (
    <div className="flex flex-col items-center gap-5">
      <img
        src="https://cdn.borderlinerp.com/f/logo_a_1K-mosqd6j9xt8cd5.png" alt="BorderlineRP"
        className="h-28 w-auto object-contain drop-shadow-[0_0_36px_rgba(214,138,60,.42)]"
        style={{ animation: "float 7s ease-in-out infinite" }}
        onError={e => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
      />
      <div className="flex flex-col items-center gap-2">
        <p key={idx} className="text-display text-[22px] text-gold-400/80" style={{ animation: "fadeIn .5s ease" }}>
          {WESTERN_MESSAGES[idx]}{dots}
        </p>
        <p className="text-mono text-[9px] uppercase tracking-[.34em] text-white/25">{label}</p>
      </div>
    </div>
  );
}

// ── app ───────────────────────────────────────────────────────────────────────

export default function App() {
  const [serverStatus, setServerStatus] = useState<ServerStatus>("loading");
  const [players, setPlayers]           = useState(0);
  const [uptime, setUptime]             = useState("--:--");
  const [discordRunning, setDiscordRunning] = useState(false);
  const [steamRunning,   setSteamRunning]   = useState(false);
  const [redmRunning,    setRedmRunning]    = useState(false);
  const [initializing, setInitializing] = useState(true);
  const [initLabel, setInitLabel]       = useState("Verifica applicazioni");
  const initDone = useRef(false);
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);
  const [steamHex, setSteamHex]         = useState<string | null>(null);
  const [steamProfile, setSteamProfile] = useState<SteamProfile | null>(null);
  const [accessStatus, setAccessStatus] = useState<AccessStatus>("loading");
  const [banInfo, setBanInfo]           = useState<BanInfo | null>(null);

  const [appVersion, setAppVersion] = useState("");
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [showUpdateModal, setShowUpdateModal] = useState(false);
  const [updateVersion, setUpdateVersion]     = useState("");
  const [updateNotes, setUpdateNotes]         = useState("");
  const [updatePhase, setUpdatePhase]         = useState<UpdatePhase>("idle");
  const [updateProgress, setUpdateProgress]   = useState(0);
  const pendingUpdate = useRef<Awaited<ReturnType<typeof checkUpdate>>>(null);

  const fetchHealth = useCallback(async () => {
    try {
      const raw  = await invoke<string>("fetch_bridge", { endpoint: "/api/v1/health" });
      const data: HealthData = JSON.parse(raw);
      if (data.success && data.data?.status === "healthy") {
        setServerStatus("online");
        setPlayers(data.data.system.players);
        setUptime(data.data.uptime.slice(0, 5));
      } else { setServerStatus("offline"); }
    } catch { setServerStatus("offline"); }
  }, []);

  const checkProcesses = useCallback(async () => {
    const { discord, steam, redm } = await invoke<{ discord: boolean; steam: boolean; redm: boolean }>("check_processes");
    setDiscordRunning(discord); setSteamRunning(steam); setRedmRunning(redm);
    return { discord, steam, redm };
  }, []);

  const recheckAccess = useCallback(async (hex: string | null) => {
    if (!hex) return;
    try {
      const raw = await invoke<string>("check_player_access", { steamHex: hex });
      const r = JSON.parse(raw);
      if (r.success && !r.isBanned) {
        setAccessStatus("allowed");
        setBanInfo(null);
      } else if (r.success && r.isBanned) {
        setAccessStatus("banned");
        setBanInfo({ reason: r.ban_reason, admin: r.ban_admin, date: r.ban_date, ban_type: r.ban_type, expires_at: r.expires_at });
      } else {
        setAccessStatus("not_allowlisted");
      }
    } catch { /* mantieni stato corrente */ }
  }, []);

  const checkForUpdates = useCallback(async () => {
    try {
      const update = await checkUpdate();
      if (update) {
        pendingUpdate.current = update;
        setUpdateVersion(update.version);
        setUpdateNotes(update.body ?? "");
        setUpdateAvailable(true);
        setShowUpdateModal(true);
      }
    } catch { /* endpoint non configurato o rete assente — silenzioso */ }
  }, []);

  const doUpdate = useCallback(async () => {
    if (!pendingUpdate.current) return;
    setUpdatePhase("downloading");
    setUpdateProgress(0);
    try {
      let downloaded = 0;
      let total = 0;
      await pendingUpdate.current.downloadAndInstall(event => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setUpdateProgress(total > 0 ? Math.round((downloaded / total) * 100) : 0);
        } else if (event.event === "Finished") {
          setUpdatePhase("done");
        }
      });
      await relaunch();
    } catch { setUpdatePhase("idle"); }
  }, []);

  const resolveIdentity = useCallback(async () => {
    setInitLabel("Identificazione Steam");
    try {
      const info = await invoke<{ hex: string; id64: string }>("get_steam_hex");
      setSteamHex(info.hex);
      setInitLabel("Recupero dati del pistolero");
      const [pr, ar] = await Promise.allSettled([
        invoke<string>("get_steam_profile", { id64: info.id64 }),
        invoke<string>("check_player_access", { steamHex: info.hex }),
      ]);
      if (pr.status === "fulfilled") {
        try {
          const p = JSON.parse(pr.value)?.response?.players?.[0];
          if (p) setSteamProfile({ name: p.personaname, avatar: p.avatarfull });
        } catch {}
      }
      if (ar.status === "fulfilled") {
        try {
          const r = JSON.parse(ar.value);
          if (r.success && !r.isBanned) { setAccessStatus("allowed"); }
          else if (r.success && r.isBanned) {
            setAccessStatus("banned");
            setBanInfo({ reason: r.ban_reason, admin: r.ban_admin, date: r.ban_date, ban_type: r.ban_type, expires_at: r.expires_at });
          } else { setAccessStatus("not_allowlisted"); }
        } catch { setAccessStatus("error"); }
      } else { setAccessStatus("error"); }
    } catch { setAccessStatus("unknown"); }
  }, []);

  useEffect(() => {
    let poll: ReturnType<typeof setInterval>;
    let tout: ReturnType<typeof setTimeout>;
    const finish = async () => { await resolveIdentity(); setInitializing(false); fetchHealth(); };
    const startup = async () => {
      setInitLabel("Verifica applicazioni");
      const { discord, steam } = await checkProcesses();
      if (discord && steam) { initDone.current = true; await finish(); return; }
      if (!discord) { setInitLabel("Avvio Discord"); await invoke("launch_discord").catch(() => {}); }
      if (!steam)   { setInitLabel("Avvio Steam");   await invoke("launch_steam").catch(() => {}); }
      setInitLabel("In attesa delle applicazioni");
      poll = setInterval(async () => {
        const { discord: d, steam: s } = await checkProcesses();
        if (d && s && !initDone.current) { initDone.current = true; clearInterval(poll); clearTimeout(tout); await finish(); }
      }, 2000);
      tout = setTimeout(async () => {
        if (!initDone.current) { initDone.current = true; clearInterval(poll); await finish(); }
      }, 90_000);
    };
    startup();
    return () => { clearInterval(poll); clearTimeout(tout); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (initializing) return;
    checkForUpdates();
    const h = setInterval(fetchHealth, 30_000);
    const p = setInterval(checkProcesses, 5_000);
    const a = setInterval(() => recheckAccess(steamHex), 30_000);
    let unlisten: (() => void) | undefined;
    listen("tauri://focus", () => recheckAccess(steamHex)).then(f => { unlisten = f; });
    return () => { clearInterval(h); clearInterval(p); clearInterval(a); unlisten?.(); };
  }, [initializing, fetchHealth, checkProcesses, recheckAccess, steamHex, checkForUpdates]);

  const canPlay =
    !redmRunning &&
    serverStatus === "online" && steamRunning &&
    (accessStatus === "allowed" || (accessStatus === "banned" && banInfo?.ban_type === "temporary"));

  const playLabel = () => {
    if (redmRunning)                 return "In gioco";
    if (serverStatus === "loading")  return "Connessione…";
    if (serverStatus === "offline")  return "Server Offline";
    if (!steamRunning)               return "Steam richiesto";
    if (accessStatus === "not_allowlisted") return "Accesso Negato";
    if (accessStatus === "banned" && banInfo?.ban_type === "permanent") return "Bannato";
    if (accessStatus === "loading")  return "Verifica…";
    return "Gioca";
  };

  // ── render ──────────────────────────────────────────────────────────────────
  return (
    <div
      className="w-[1100px] h-[680px] flex flex-col overflow-hidden select-none"
      style={{ background: "#060402" }}
      data-mood="frontier"
    >
      <style>{KEYFRAMES}</style>

      {/* ══ title bar ══ */}
      <header
        className="h-9 shrink-0 flex items-center justify-between px-4 z-30"
        style={{ background: "#060402", backdropFilter: "blur(14px)" }}
        data-tauri-drag-region
      >
        <div className="flex items-center gap-2" data-tauri-drag-region>
          <Diamond cls="bg-gold-500/60" />
          <span className="text-mono text-[9px] uppercase tracking-[.40em] text-gold-500/45">Borderline Launcher</span>
        </div>
        <div className="flex items-center gap-2">
          {updateAvailable && (
            <button
              onClick={() => setShowUpdateModal(true)}
              className="flex items-center gap-1.5 px-2 py-0.5 border border-gold-600/40 hover:border-gold-500/70 transition-colors cursor-pointer"
              style={{ background: "rgba(201,161,74,.10)", animation: "emporio-glow 2.5s ease-in-out infinite" }}
            >
              <span className="w-1 h-1 rounded-full bg-gold-400 animate-pulse flex-shrink-0" />
              <span className="text-mono text-[7px] uppercase tracking-[.28em] text-gold-400">v{updateVersion}</span>
            </button>
          )}
          <div className="flex items-center gap-0.5">
            <button onClick={() => win().minimize()} className="w-7 h-7 flex items-center justify-center text-white/25 hover:text-white/80 hover:bg-white/8 transition-colors text-[13px] font-thin cursor-pointer">―</button>
            <button onClick={() => win().hide()}     className="w-7 h-7 flex items-center justify-center text-white/25 hover:text-blood-600 hover:bg-blood-600/20 transition-colors text-[11px] cursor-pointer">✕</button>
          </div>
        </div>
      </header>

      {/* ══ main ══ */}
      <div className="flex-1 relative overflow-hidden">

        {/* background */}
        <img
          src="https://cdn.borderlinerp.com/f/banner-mosqd6hsgitgm8.gif" alt="" aria-hidden
          className="absolute inset-0 w-full h-full object-cover pointer-events-none"
          style={{ opacity: .68, filter: "sepia(.18) brightness(.72) contrast(1.04)" }}
        />

        {/* vignette — leggero al centro, scuro in alto e in basso */}
        <div className="absolute inset-0 pointer-events-none" style={{
          background: [
            "linear-gradient(to bottom, rgba(4,3,2,.48) 0%, transparent 34%, transparent 50%, rgba(4,3,2,.96) 100%)",
            "radial-gradient(ellipse 75% 65% at 50% 42%, transparent 0%, rgba(4,3,2,.18) 70%, rgba(4,3,2,.52) 100%)",
          ].join(", "),
        }} />

        {/* ── TOP-LEFT CARD — server + apps ── */}
        <div
          className="absolute top-3 left-4 z-20 flex flex-col gap-2.5 p-3.5 border border-white/8"
          style={{
            width: 200,
            background: "rgba(4,3,2,.72)",
            backdropFilter: "blur(18px)",
            animation: "card-in .4s ease both",
          }}
        >
          {/* server */}
          <div className="flex items-center gap-2">
            <Pip status={serverStatus} />
            <div className="min-w-0">
              <span className="text-mono text-[9px] uppercase tracking-widest text-bone-100/75">
                {serverStatus === "online" ? "Online" : serverStatus === "offline" ? "Offline" : "Verifica…"}
              </span>
              {serverStatus === "online" && (
                <span className="text-mono text-[8px] text-white/30 ml-1.5">{players} gioc · {uptime}</span>
              )}
            </div>
          </div>

          {/* separator */}
          <div className="h-px bg-white/7" />

          {/* apps */}
          <div className="flex flex-col gap-1.5">
            {[
              { label: "Discord", running: discordRunning, cmd: "launch_discord" },
              { label: "Steam",   running: steamRunning,   cmd: "launch_steam"   },
            ].map(app => (
              <div key={app.label} className="flex items-center justify-between">
                <div className="flex items-center gap-1.5">
                  <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 transition-colors ${app.running ? "bg-moss-500" : "bg-blood-600/70"}`} />
                  <span className="text-body text-[11px] text-bone-200/65">{app.label}</span>
                </div>
                {app.running
                  ? <span className="text-mono text-[8px] text-moss-500/55 uppercase tracking-wider">Attivo</span>
                  : <button onClick={() => invoke(app.cmd)} className="text-mono text-[8px] uppercase tracking-wider text-gold-500/55 hover:text-gold-400 transition-colors cursor-pointer">Avvia →</button>
                }
              </div>
            ))}
          </div>
        </div>

        {/* ── TOP-RIGHT CARD — profilo + accesso ── */}
        <div
          className="absolute top-3 right-4 z-20 flex flex-col gap-2.5 p-3.5 border border-white/8"
          style={{
            width: 195,
            background: "rgba(4,3,2,.72)",
            backdropFilter: "blur(18px)",
            animation: "card-in .4s ease .05s both",
          }}
        >
          {/* profilo Steam */}
          {steamProfile ? (
            <div className="flex items-center gap-2.5">
              <img src={steamProfile.avatar} alt={steamProfile.name} className="w-8 h-8 object-cover border border-gold-700/30 flex-shrink-0" />
              <div className="min-w-0">
                <div className="text-body text-[12px] text-bone-100/85 truncate">{steamProfile.name}</div>
                {steamHex && <div className="text-mono text-[7px] text-white/22 truncate mt-0.5">{steamHex.slice(0,6)}…{steamHex.slice(-4)}</div>}
              </div>
            </div>
          ) : (
            <div className="flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-white/20 animate-pulse flex-shrink-0" />
              <span className="text-mono text-[8px] text-white/30 uppercase tracking-wider">Steam…</span>
            </div>
          )}

          {/* separator */}
          <div className="h-px bg-white/7" />

          {/* accesso */}
          <AccessChip status={accessStatus} ban={banInfo} />
        </div>

        {/* ── CENTER HERO ── */}
        <div className="absolute inset-0 flex flex-col items-center justify-center pb-10">
          {initializing ? (
            <LoadingCenter label={initLabel} />
          ) : (
            <div className="flex flex-col items-center gap-6">
              {/* logo + titolo */}
              <div className="flex flex-col items-center gap-1.5" style={{ animation: "fade-up .55s ease both" }}>
                {/* <img
                  src="https://cdn.borderlinerp.com/f/logo_a_1K-mosqd6j9xt8cd5.png" alt="BorderlineRP"
                  className="h-[130px] w-auto object-contain drop-shadow-[0_0_48px_rgba(214,138,60,.40)]"
                  onError={e => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
                /> */}
                <h1 className="text-display uppercase text-[60px] leading-[.9] text-bone-50 drop-shadow-[0_4px_24px_rgba(0,0,0,.95)]">
                  Borderline
                </h1>
                <div className="flex items-center gap-3 mt-1">
                  <span className="h-px w-12 bg-gradient-to-r from-transparent to-gold-500/38" />
                  <span className="text-mono text-[9px] tracking-[.32em] text-gold-400 uppercase">Il tuo viaggio nel West ha inizio qui</span>
                  <span className="h-px w-12 bg-gradient-to-l from-transparent to-gold-500/38" />
                </div>
              </div>

              {/* pulsante GIOCA */}
              <div className="flex flex-col items-center gap-3" style={{ animation: "fade-up .55s ease .08s both" }}>
                <button
                  onClick={async () => {
                    if (!canPlay || !steamHex) return;
                    try { await invoke("authorize_entry", { steamHex }); } catch { /* non bloccare il gioco se la chiamata fallisce */ }
                    invoke("launch_game");
                  }}
                  disabled={!canPlay}
                  className={`
                    western-border relative overflow-hidden
                    w-[380px] py-6 text-display text-[40px] tracking-[.22em]
                    transition-all duration-300
                    ${canPlay
                      ? "bg-linear-to-r from-gold-600 to-gold-400 text-ink-900 hover:scale-105 active:scale-[.96] cursor-pointer"
                      : "bg-linear-to-r from-ink-900/85 to-ink-700/85  text-blood-600 cursor-not-allowed border border-white/6"}
                  `}
                  style={canPlay ? { animation: "glow-pulse 2.8s ease-in-out infinite" } : undefined}
                >
                  {canPlay && (
                    <span className="absolute inset-0 overflow-hidden pointer-events-none">
                      <span className="absolute inset-y-0 w-1/4"
                        style={{ background: "linear-gradient(90deg,transparent,rgba(255,255,255,.14),transparent)", animation: "shimmer-x 2.6s ease-in-out infinite" }} />
                    </span>
                  )}
                  <span className="relative uppercase">{canPlay ? "▶ " : ""}{playLabel()}</span>
                </button>

                {/* hint */}
                <div className="h-4 flex items-center justify-center">
                  {accessStatus === "banned" && banInfo?.ban_type === "temporary" && (
                    <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Ban temporaneo · istanza separata</span>
                  )}
                  {accessStatus === "not_allowlisted" && (
                    <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Apri un ticket Discord per la whitelist</span>
                  )}
                  {!steamRunning && serverStatus === "online" && (
                    <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Steam non rilevato — richiesto per giocare</span>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>

        {/* ══ UPDATE MODAL ══ */}
        {showUpdateModal && (
          <UpdateModal
            version={updateVersion}
            notes={updateNotes}
            phase={updatePhase}
            progress={updateProgress}
            onUpdate={doUpdate}
            onDismiss={() => setShowUpdateModal(false)}
          />
        )}

        {/* ══ BOTTOM BAR ══ */}
        <div
          className="absolute bottom-0 left-0 right-0 h-14 grid items-center z-20 border-t border-white/6 px-5"
          style={{
            gridTemplateColumns: "1fr auto 1fr",
            background: "rgba(4,3,2,.85)",
            backdropFilter: "blur(16px)",
          }}
        >
          {/* LEFT — meta + status */}
          <div className="flex items-center gap-3">
            <span className="text-mono text-[8px] uppercase tracking-[.28em] text-white/20">{appVersion ? `v${appVersion}` : ""}</span>
            <span className="text-white/12">·</span>
            <span className="text-mono text-[8px] text-white/16">© 2026 BorderlineRP</span>
            {serverStatus === "online" && !initializing && (
              <>
                <span className="text-white/12">·</span>
                <span className="flex items-center gap-1.5">
                  <span className="w-1 h-1 rounded-full bg-moss-500" />
                  <span className="text-mono text-[8px] text-moss-500/55 uppercase tracking-wider">Online</span>
                </span>
              </>
            )}
          </div>

          {/* CENTER — EMPORIO */}
          <button
            onClick={() => openUrl("https://emporio.borderlinerp.com")}
            className="group relative overflow-hidden flex items-center gap-3 px-6 py-2 border border-gold-600/35 hover:border-gold-500/65 transition-colors duration-300 cursor-pointer"
            style={{
              background: "linear-gradient(135deg, rgba(201,161,74,.08) 0%, rgba(139,90,30,.04) 100%)",
              animation: "emporio-glow 3s ease-in-out infinite",
            }}
          >
            <span className="absolute inset-0 overflow-hidden pointer-events-none">
              <span className="absolute inset-y-0 w-1/3 opacity-0 group-hover:opacity-100 transition-opacity duration-300"
                style={{ background: "linear-gradient(90deg,transparent,rgba(201,161,74,.14),transparent)", animation: "shimmer-x 2s ease-in-out infinite" }} />
            </span>
            <Diamond cls="bg-gold-500/55 group-hover:bg-gold-400 transition-colors" />
            <span className="text-display text-[18px] text-gold-400 group-hover:text-gold-300 tracking-widest transition-colors relative">
              Emporio di Borderline
            </span>
            <span className="text-gold-500/35 group-hover:text-gold-400/80 group-hover:translate-x-0.5 transition-all text-[12px] relative">→</span>
          </button>

          {/* RIGHT — link + aggiorna */}
          <div className="flex items-center justify-end gap-1">
            {[
              { label: "Sito",    url: "https://borderlinerp.com" },
              { label: "Discord", url: "https://discord.borderlinerp.com" },
            ].map(link => (
              <button
                key={link.url}
                onClick={() => openUrl(link.url)}
                className="flex items-center gap-1.5 px-3 py-1.5 text-bone-200/38 hover:text-bone-100/75 hover:bg-white/5 transition-all cursor-pointer group"
              >
                <span className="text-mono text-[9px] uppercase tracking-wider">{link.label}</span>
                <span className="text-[9px] text-white/18 group-hover:text-white/50 group-hover:translate-x-0.5 transition-all">→</span>
              </button>
            ))}
            <div className="w-px h-4 bg-white/8 mx-2" />
            <button
              onClick={() => { fetchHealth(); checkProcesses(); resolveIdentity(); }}
              className="text-mono text-[8px] uppercase tracking-[.28em] text-white/18 hover:text-white/45 transition-colors cursor-pointer px-2"
            >
              ↻ Aggiorna
            </button>
          </div>
        </div>

      </div>
    </div>
  );
}

// ── UpdateModal ───────────────────────────────────────────────────────────────

type UpdatePhase = "idle" | "downloading" | "done";

function UpdateModal({
  version, notes, phase, progress, onUpdate, onDismiss,
}: {
  version: string; notes: string; phase: UpdatePhase;
  progress: number; onUpdate: () => void; onDismiss: () => void;
}) {
  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(4,3,2,.88)", backdropFilter: "blur(10px)" }}
    >
      <div
        className="flex flex-col gap-5 p-7 border border-gold-600/35"
        style={{ width: 440, background: "rgba(8,6,3,.96)", animation: "card-in .3s ease" }}
      >
        {/* header */}
        <div className="flex flex-col gap-1.5">
          <span className="text-mono text-[8px] uppercase tracking-[.36em] text-gold-500/55">Aggiornamento Disponibile</span>
          <h2 className="text-display text-[38px] leading-none text-bone-50">v{version}</h2>
        </div>

        <div className="h-px bg-white/8" />

        {/* note di rilascio */}
        {notes && (
          <p className="text-body text-[11px] text-bone-200/55 leading-relaxed max-h-28 overflow-y-auto whitespace-pre-wrap">{notes}</p>
        )}

        {/* progress */}
        {phase === "downloading" && (
          <div className="flex flex-col gap-2">
            <div className="h-0.5 bg-white/8 overflow-hidden">
              <div
                className="h-full bg-gold-500 transition-all duration-200"
                style={{ width: `${progress}%` }}
              />
            </div>
            <span className="text-mono text-[8px] uppercase tracking-wider text-white/28">
              Download {progress}%
            </span>
          </div>
        )}

        {phase === "done" && (
          <span className="text-mono text-[8px] uppercase tracking-wider text-moss-500">
            Installazione completata — riavvio in corso…
          </span>
        )}

        {/* azioni */}
        {phase === "idle" && (
          <div className="flex items-center gap-3 pt-1">
            <button
              onClick={onUpdate}
              className="flex-1 py-3.5 text-display text-[20px] tracking-[.22em] text-ink-900 bg-gold-500 hover:bg-gold-400 transition-colors cursor-pointer"
              style={{ animation: "glow-pulse 2.8s ease-in-out infinite" }}
            >
              AGGIORNA ORA
            </button>
            <button
              onClick={onDismiss}
              className="px-5 py-3.5 text-mono text-[8px] uppercase tracking-wider text-white/30 hover:text-white/55 border border-white/10 hover:border-white/22 transition-colors cursor-pointer"
            >
              Più Tardi
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ── AccessChip (inline nella card top-right) ──────────────────────────────────

function AccessChip({ status, ban }: { status: AccessStatus; ban: BanInfo | null }) {
  if (status === "loading")
    return <div className="flex items-center gap-1.5"><span className="w-1.5 h-1.5 rounded-full bg-white/20 animate-pulse flex-shrink-0" /><span className="text-mono text-[8px] text-white/30 uppercase tracking-wider">Verifica…</span></div>;

  if (status === "allowed")
    return (
      <div className="flex items-center gap-1.5">
        <Diamond cls="bg-moss-500" />
        <span className="text-mono text-[8px] text-moss-500 font-bold uppercase tracking-wider">Hai la whitelist</span>
      </div>
    );

  if (status === "not_allowlisted")
    return (
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-1.5">
          <Diamond cls="bg-blood-600" />
          <span className="text-mono text-[8px] text-blood-400 uppercase tracking-wider">Non Allowlistato</span>
        </div>
        <button onClick={() => openUrl("https://discord.borderlinerp.com")} className="text-mono text-[8px] text-gold-500/55 hover:text-gold-400 transition-colors cursor-pointer uppercase tracking-wider">→ Apri Discord</button>
      </div>
    );

  if (status === "banned") {
    const isPerm = ban?.ban_type === "permanent";
    const expires = ban?.expires_at
      ? new Date(ban.expires_at).toLocaleDateString("it-IT", { day: "2-digit", month: "2-digit", year: "2-digit" })
      : null;
    return (
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-1.5">
          <Diamond cls="bg-blood-600" />
          <span className="text-mono text-[8px] text-blood-400 uppercase tracking-wider">{isPerm ? "Ban Permanente" : "Ban Temporaneo"}</span>
        </div>
        {ban?.reason && <p className="text-[9px] text-white/45 font-body leading-snug">{ban.reason}</p>}
        {expires && <p className="text-mono text-[7px] text-blood-500/55">Scade: {expires}</p>}
        <button onClick={() => openUrl("https://discord.borderlinerp.com")} className="text-mono text-[8px] text-gold-500/55 hover:text-gold-400 transition-colors cursor-pointer uppercase tracking-wider">→ Ricorso Discord</button>
      </div>
    );
  }

  if (status === "unknown")
    return (
      <div className="flex items-center gap-1.5">
        <Diamond cls="bg-white/20" />
        <span className="text-mono text-[8px] text-white/28 uppercase tracking-wider">Steam offline</span>
      </div>
    );

  return (
    <div className="flex items-center gap-1.5">
      <Diamond cls="bg-gold-700/50" />
      <span className="text-mono text-[8px] text-white/28 uppercase tracking-wider">Allowlist</span>
    </div>
  );
}
