import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PlayButton } from "./PlayButton";
import LivingBackground from "./LivingBackground";
import { Poster, PaperHeader, PaperRule, Star, InkStamp, INK, PAPER_NOISE } from "./western";
import { VoiceGuideModal, voiceBlocked, type VoicePlugin } from "./VoiceGuide";
import { Copy, Check, Download } from "lucide-react";
import { motion } from "motion/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { relaunch, exit } from "@tauri-apps/plugin-process";
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

// Server TeamSpeak del territorio — mostrato sotto la checklist, copiabile.
const TS3_ADDRESS = "ts3dev.borderlinerp.com";

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
  @keyframes ink-stamp  { 0%{transform:rotate(var(--stamp-rot,-7deg)) scale(1.6);opacity:0} 60%{transform:rotate(var(--stamp-rot,-7deg)) scale(.94);opacity:.92} 100%{transform:rotate(var(--stamp-rot,-7deg)) scale(1);opacity:.88} }
  @keyframes paper-sway { 0%,100%{transform:rotate(-1.2deg)} 50%{transform:rotate(.8deg)} }
  @keyframes emporio-sweep { 0%{transform:translateX(-100%)} 100%{transform:translateX(420%)} }
  .emporio-shine { left: 0; opacity: 0; transform: translateX(-100%); }
  .emporio-btn:hover .emporio-shine { opacity: 1; animation: emporio-sweep 1.5s linear infinite; }
`;

// ── primitivi ─────────────────────────────────────────────────────────────────

function Diamond({ cls = "" }: { cls?: string }) {
  return <span className={`block w-1.5 h-1.5 rotate-45 flex-shrink-0 ${cls}`} />;
}

// ── titolo animato — onda + bagliore dorato che viaggia, loop infinito ──────────

const TITLE_TEXT = "Borderline";
const TITLE_DARK = "0 4px 24px rgba(0,0,0,.95)";

function AnimatedTitle() {
  return (
    <h1
      aria-label={TITLE_TEXT}
      className="text-display uppercase text-[60px] leading-[.9] inline-flex"
      style={{ color: "#f3e2bd" }}
    >
      {TITLE_TEXT.split("").map((ch, i) => (
        <motion.span
          key={i}
          aria-hidden
          className="inline-block"
          style={{ willChange: "transform" }}
          animate={{
            y: [0, -9, 0],
            color: ["#f3e2bd", "#efbe85", "#f3e2bd"],
            textShadow: [
              `${TITLE_DARK}, 0 0 0px rgba(230,164,92,0)`,
              `${TITLE_DARK}, 0 0 22px rgba(230,164,92,.75)`,
              `${TITLE_DARK}, 0 0 0px rgba(230,164,92,0)`,
            ],
          }}
          transition={{
            duration: 2.6,
            ease: "easeInOut",
            repeat: Infinity,
            repeatDelay: 0.6,
            delay: i * 0.14,
          }}
        >
          {ch}
        </motion.span>
      ))}
    </h1>
  );
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
  const playersRef = useRef(0);
  const [uptime, setUptime]             = useState("--:--");
  const [discordRunning, setDiscordRunning] = useState(false);
  const [steamRunning,   setSteamRunning]   = useState(false);
  const [redmRunning,    setRedmRunning]    = useState(false);
  const [teamspeakRunning, setTeamspeakRunning] = useState(false);
  const [voicePlugin, setVoicePlugin] = useState<VoicePlugin>({ status: "checking" });
  const [showVoiceGuide, setShowVoiceGuide] = useState(false);
  const checkVoicePlugin = useCallback(async () => {
    try { setVoicePlugin(await invoke<VoicePlugin>("ensure_voice_plugin")); }
    catch (e) { setVoicePlugin({ status: "error", message: String(e) }); }
  }, []);
  const [initializing, setInitializing] = useState(true);
  const [initLabel, setInitLabel]       = useState("Verifica applicazioni");
  const initDone = useRef(false);
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);
  const [steamHex, setSteamHex]         = useState<string | null>(null);
  const [steamProfile, setSteamProfile] = useState<SteamProfile | null>(null);
  const [accessStatus, setAccessStatus] = useState<AccessStatus>("loading");
  const [banInfo, setBanInfo]           = useState<BanInfo | null>(null);
  // authenticated = challenge del server verificato (client integro e presente).
  const [hbStatus, setHbStatus] = useState<{ authenticated: boolean }>({ authenticated: false });
  const [playError, setPlayError] = useState<string | null>(null);
  // Avviso sull'istanza separata, mostrato prima dell'avvio a chi ha un ban temporaneo.
  const [showExileModal, setShowExileModal] = useState(false);

  const [showSplash, setShowSplash] = useState(true);
  const [splashExiting, setSplashExiting] = useState(false);
  const [showCloseModal, setShowCloseModal] = useState(false);
  const [keepInBackground, setKeepInBackground] = useState(true);

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
        playersRef.current = data.data.system.players;
        setUptime(data.data.uptime.slice(0, 5));
      } else { setServerStatus("offline"); }
    } catch { setServerStatus("offline"); }
  }, []);

  // Conteggio aggregato (calcolato dal thread anti-cheat sull'ultimo heartbeat)
  // → Discord Rich Presence. %players = giocatori in-game + launcher aperti.
  // L'heartbeat HTTP non è più qui: lo possiede il thread Rust (scan + challenge).
  const updatePresence = useCallback(async () => {
    let total = playersRef.current;
    try {
      const r = await invoke<{ players: number; launchers: number }>("get_heartbeat_counts");
      total = (r.players || playersRef.current) + (r.launchers || 0);
    } catch { /* stato non ancora pronto — usa il fallback in-game */ }
    invoke("update_discord_presence", { players: total }).catch(() => { /* Discord non attivo */ });
  }, []);

  const checkProcesses = useCallback(async () => {
    const { discord, steam, redm, teamspeak } = await invoke<{ discord: boolean; steam: boolean; redm: boolean; teamspeak: boolean }>("check_processes");
    setDiscordRunning(discord); setSteamRunning(steam); setRedmRunning(redm); setTeamspeakRunning(teamspeak);
    return { discord, steam, redm, teamspeak };
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

  // Polling stato anti-cheat (autenticazione challenge + rilevazione dumper).
  const checkHeartbeat = useCallback(async () => {
    try {
      const r = await invoke<{ authenticated: boolean }>("get_heartbeat_status");
      setHbStatus({ authenticated: !!r.authenticated });
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
      } else {
        console.info("[updater] nessun aggiornamento disponibile");
      }
    } catch (e) {
      // permessi mancanti, endpoint irraggiungibile, firma non valida, ecc.
      console.error("[updater] check fallito:", e);
    }
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
    } catch (e) {
      console.error("[updater] download/install fallito:", e);
      setUpdatePhase("idle");
    }
  }, []);

  const resolveIdentity = useCallback(async () => {
    setInitLabel("Identificazione Steam");
    try {
      const info = await invoke<{ hex: string; id64: string }>("get_steam_hex");
      setSteamHex(info.hex);
      // Comunica l'identità al thread anti-cheat: da qui parte l'heartbeat
      // (scan dumper + challenge) gestito interamente lato Rust.
      invoke("set_launcher_identity", { steamHex: info.hex }).catch(() => {});
      setInitLabel("Recupero dati del pioniere");
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
    const finish = async () => { await resolveIdentity(); setInitializing(false); fetchHealth(); checkHeartbeat(); };
    const startup = async () => {
      setInitLabel("Verifica applicazioni");
      const { discord, steam, teamspeak } = await checkProcesses();
      // Plugin vocale prima di avviare TeamSpeak: a TS3 chiuso si installa subito.
      setInitLabel("Aggiornamento plugin vocale");
      await checkVoicePlugin();
      setInitLabel("Verifica applicazioni");
      // TeamSpeak è opzionale (non blocca l'init): fire-and-forget se non gira.
      if (!teamspeak) { invoke("launch_teamspeak").catch(() => {}); }
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
    const t = setTimeout(() => {
      setSplashExiting(true);
      setTimeout(() => setShowSplash(false), 600);
    }, 1500);
    return () => clearTimeout(t);
  }, []);

  const handleClose = useCallback(async () => {
    setShowCloseModal(false);
    if (keepInBackground) {
      await win().hide();
    } else {
      await exit(0);
    }
  }, [keepInBackground]);

  useEffect(() => {
    if (initializing) return;
    checkForUpdates();
    updatePresence();
    checkHeartbeat();
    const h = setInterval(fetchHealth, 30_000);
    const p = setInterval(checkProcesses, 5_000);
    const a = setInterval(() => recheckAccess(steamHex), 30_000);
    const d = setInterval(updatePresence, 30_000);
    const c = setInterval(checkHeartbeat, 3_000);
    const u = setInterval(() => { checkForUpdates(); checkVoicePlugin(); }, 15 * 60_000); // check updater + plugin vocale ogni 15 min
    let unlisten: (() => void) | undefined;
    listen("tauri://focus", () => recheckAccess(steamHex)).then(f => { unlisten = f; });
    return () => { clearInterval(h); clearInterval(p); clearInterval(a); clearInterval(d); clearInterval(c); clearInterval(u); unlisten?.(); };
  }, [initializing, fetchHealth, checkProcesses, recheckAccess, steamHex, checkForUpdates, updatePresence, checkHeartbeat]);

  useEffect(() => {
    if (initializing) return;
    if (voiceBlocked(voicePlugin.status)) setShowVoiceGuide(true);
    else if (voicePlugin.status === "ok" || voicePlugin.status === "installed") {
      if (voicePlugin.teamspeak_running ?? teamspeakRunning) setShowVoiceGuide(false);
    }
  }, [voicePlugin.status, voicePlugin.teamspeak_running, teamspeakRunning, initializing]);

  const canPlay =
    !redmRunning && !voiceBlocked(voicePlugin.status) &&
    serverStatus === "online" && steamRunning && teamspeakRunning && discordRunning &&
    (accessStatus === "allowed" || (accessStatus === "banned" && banInfo?.ban_type === "temporary")) &&
    hbStatus.authenticated;

  const playLabel = () => {
    if (redmRunning)                 return "In gioco";
    if (serverStatus === "loading")  return "Connessione…";
    if (serverStatus === "offline")  return "Server Offline";
    if (!steamRunning)               return "Steam richiesto";
    if (!teamspeakRunning)           return "TeamSpeak richiesto";
    if (!discordRunning)             return "Discord richiesto";
    if (voiceBlocked(voicePlugin.status)) return "Addon vocale da sistemare";
    if (accessStatus === "not_allowlisted") return "Accesso Negato";
    if (accessStatus === "banned" && banInfo?.ban_type === "permanent") return "Bannato";
    if (accessStatus === "loading")  return "Verifica…";
    if (!hbStatus.authenticated)     return "Autenticazione…";
    return "Gioca";
  };

  // Un avvio fallito (schema URL non registrato, app disinstallata) deve dirlo:
  // altrimenti il giocatore preme e non succede nulla. `spawn_detached` lato Rust
  // ritorna un messaggio già leggibile, lo mostriamo nella riga di hint.
  const launch = useCallback((cmd: string) => {
    setPlayError(null);
    invoke(cmd).catch(e => setPlayError(String(e)));
  }, []);

  // Sequenza d'avvio: verifica di sicurezza — autorizzazione — lancio di RedM.
  const startGame = useCallback(async () => {
    if (!steamHex) return;
    setPlayError(null);
    // 1) Ricontrolla l'autenticazione del launcher appena prima dell'avvio.
    try {
      const ac = await invoke<{ authenticated: boolean }>("get_heartbeat_status");
      if (!ac.authenticated) { setPlayError("Autenticazione in corso, riprova tra un istante."); checkHeartbeat(); return; }
    } catch { setPlayError("Verifica di sicurezza non riuscita."); return; }
    // 2) Autorizza l'ingresso e verifica l'esito: NIENTE avvio se fallisce.
    try {
      const raw = await invoke<string>("authorize_entry", { steamHex });
      const r = JSON.parse(raw);
      if (!r.success) { setPlayError(r.message || "Autorizzazione negata dal server."); return; }
    } catch { setPlayError("Impossibile contattare il server. Riprova."); return; }
    // 3) Tutto verificato → avvia RedM.
    invoke("launch_game").catch(e => setPlayError(String(e)));
  }, [steamHex, checkHeartbeat]);

  // ── render ──────────────────────────────────────────────────────────────────
  return (
    <div
      className="relative w-[1100px] h-[680px] flex flex-col overflow-hidden select-none"
      style={{ background: "#060402" }}
      data-mood="frontier"
    >
      <style>{KEYFRAMES}</style>
      {showSplash && <SplashScreen exiting={splashExiting} />}
      {showCloseModal && (
        <CloseModal
          keepInBackground={keepInBackground}
          onToggle={() => setKeepInBackground(v => !v)}
          onConfirm={handleClose}
          onCancel={() => setShowCloseModal(false)}
        />
      )}

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
              <span className="text-mono text-[7px] uppercase tracking-[.28em] text-gold-400">AGGIORNAMENTO DISPONIBILE</span>
            </button>
          )}
          <div className="flex items-center gap-0.5">
            <button onClick={() => win().minimize()} className="w-7 h-7 flex items-center justify-center text-white/25 hover:text-white/80 hover:bg-white/8 transition-colors text-[13px] font-thin cursor-pointer">―</button>
            <button onClick={() => setShowCloseModal(true)} className="w-7 h-7 flex items-center justify-center text-white/25 hover:text-blood-600 hover:bg-blood-600/20 transition-colors text-[11px] cursor-pointer">✕</button>
          </div>
        </div>
      </header>

      {/* ══ main ══ */}
      <div className="flex-1 relative overflow-hidden">

        {/* ── SFONDO VIVO — braci, fumo, raggio, parallax (sostituisce lo statico) ── */}
        <LivingBackground centerDark={canPlay ? 0.42 : 0.6} />

        {/* ── TOP-LEFT — bollettino del territorio (server + apps) ── */}
        <div className="absolute top-4 left-5 z-20" style={{ animation: "card-in .4s ease both" }}>
          <Poster width={212} pinned>
            <PaperHeader no="Territorio di Borderline" title="Bollettino" />

            {/* stato del territorio — stella + timbro */}
            <div className="flex flex-col items-center gap-1 mt-2.5 mb-1">
              <Star size={26} style={{ color: serverStatus === "online" ? INK.green : serverStatus === "offline" ? INK.red : INK.soft }} />
              <InkStamp
                key={serverStatus}
                color={serverStatus === "online" ? INK.green : serverStatus === "offline" ? INK.red : INK.soft}
                size={26}
              >
                {serverStatus === "online" ? "APERTO" : serverStatus === "offline" ? "CHIUSO" : "VERIFICA…"}
              </InkStamp>
            </div>

            {/* conteggio */}
            {serverStatus === "online" ? (
              <>
                <p className="text-serif-sc text-center text-[12px] leading-snug mt-1.5" style={{ color: INK.text }}>
                  <span className="text-display text-[18px]" style={{ color: INK.red }}>{players}</span> {players === 1 ? "pioniere" : "pionieri"} in città
                </p>
                <div className="text-mono text-[7px] text-center uppercase tracking-[.28em] mt-1.5" style={{ color: INK.faint }}>
                  in sella da {uptime}
                </div>
              </>
            ) : (
              <p className="text-serif-sc text-center text-[11px] mt-1.5" style={{ color: INK.soft }}>
                Nessun bollettino dal fronte
              </p>
            )}

            <PaperRule double className="my-3" />

            {/* applicazioni — checklist d'inchiostro */}
            <div className="flex flex-col gap-1.5">
              {[
                { label: "Discord",   running: discordRunning,   action: () => launch("launch_discord"),   missing: "✗ non avviato" },
                { label: "Steam",     running: steamRunning,     action: () => launch("launch_steam"),     missing: "✗ non avviato" },
                { label: "TeamSpeak", running: teamspeakRunning, action: () => launch("launch_teamspeak"), missing: "✗ non avviato" },
              ].map(app => (
                <div key={app.label} className="flex items-center justify-between">
                  <span className="text-serif-sc text-[12px] tracking-wide" style={{ color: INK.text }}>{app.label}</span>
                  {app.running
                    ? <span className="text-display text-[13px]" style={{ color: INK.green, transform: "rotate(-5deg)", display: "inline-block" }}>✓ pronto</span>
                    : <button onClick={app.action} className="text-display text-[13px] cursor-pointer hover:opacity-70 transition-opacity" style={{ color: INK.red, transform: "rotate(-5deg)" }}>{app.missing}</button>
                  }
                </div>
              ))}
              <VoicePluginRow state={voicePlugin} retry={() => (voiceBlocked(voicePlugin.status) ? setShowVoiceGuide(true) : checkVoicePlugin())} />
              <Ts3Copy />
            </div>

            {/* addon vocale: forza il controllo/installazione di Borderline Voice (vedi ensure_voice_plugin) */}
            <button
              onClick={() => (voiceBlocked(voicePlugin.status) ? setShowVoiceGuide(true) : checkVoicePlugin())}
              title="Controlla e installa l'addon vocale Borderline Voice in TeamSpeak"
              className="group mt-3 w-full flex items-center justify-center gap-2 py-2 cursor-pointer transition-all hover:-translate-y-px active:translate-y-0"
              style={{ background: INK.red, color: "#f3e2bd", boxShadow: "0 3px 0 rgba(60,18,8,.55)" }}
            >
              <Download size={13} className="group-hover:translate-y-px transition-transform" />
              <span className="text-display text-[13px] uppercase tracking-[.1em]">Addon vocale</span>
            </button>
          </Poster>
        </div>

        {/* ── TOP-RIGHT — schedario del pioniere (profilo + accesso) ── */}
        <div className="absolute top-4 right-5 z-20" style={{ animation: "card-in .4s ease .05s both" }}>
          <Poster width={208} pinned>
            <PaperHeader no="Schedario del Pioniere" title="Identità" />

            {/* mugshot */}
            {steamProfile ? (
              <div className="flex items-center gap-3 mt-3">
                <div className="relative flex-shrink-0" style={{ padding: 3, background: "#2a1c0e" }}>
                  <img src={steamProfile.avatar} alt={steamProfile.name} className="w-11 h-11 object-cover" style={{ filter: "sepia(.45) contrast(1.05)" }} />
                </div>
                <div className="min-w-0">
                  <div className="text-serif-sc text-[13px] leading-tight break-words line-clamp-2" style={{ color: INK.text }} title={steamProfile.name}>{steamProfile.name}</div>
                  {steamHex && <HexCopy hex={steamHex} />}
                </div>
              </div>
            ) : (
              <div className="flex items-center justify-center gap-2 mt-3 py-1">
                <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: INK.soft }} />
                <span className="text-serif-sc text-[11px] tracking-wide" style={{ color: INK.soft }}>Identificazione…</span>
              </div>
            )}

            <PaperRule double className="my-3" />

            {/* accesso */}
            <AccessChip status={accessStatus} ban={banInfo} />
          </Poster>
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
                <AnimatedTitle />
                <div className="flex items-center gap-3 mt-1.5">
                  <span className="h-px w-10 bg-gradient-to-r from-transparent to-gold-500/45" />
                  <Star size={9} style={{ color: "var(--accent)" }} className="opacity-80" />
                  <span className="text-mono text-[9px] tracking-[.32em] text-gold-400 uppercase">Il tuo viaggio nel West ha inizio qui</span>
                  <Star size={9} style={{ color: "var(--accent)" }} className="opacity-80" />
                  <span className="h-px w-10 bg-gradient-to-l from-transparent to-gold-500/45" />
                </div>
              </div>

              {/* pulsante GIOCA */}
              <div className="flex flex-col items-center gap-3" style={{ animation: "fade-up .55s ease .08s both" }}>
                <PlayButton
                  canPlay={canPlay}
                  label={playLabel()}
                  onClick={() => {
                    if (!canPlay || !steamHex) return;
                    // Ban temporaneo: prima spiega l'istanza separata, si avvia da lì.
                    if (accessStatus === "banned" && banInfo?.ban_type === "temporary") { setShowExileModal(true); return; }
                    startGame();
                  }}
                />

                {/* hint */}
                <div className="h-4 flex items-center justify-center">
                  {playError ? (
                    <span className="text-mono text-[8px] text-blood-500/80 uppercase tracking-wider">{playError}</span>
                  ) : (
                    <>
                      {accessStatus === "banned" && banInfo?.ban_type === "temporary" && (
                        <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Ban temporaneo · istanza separata</span>
                      )}
                      {accessStatus === "not_allowlisted" && (
                        <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Apri un ticket Discord per la whitelist</span>
                      )}
                      {!steamRunning && serverStatus === "online" && (
                        <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Steam non rilevato — richiesto per giocare</span>
                      )}
                      {steamRunning && !teamspeakRunning && serverStatus === "online" && (
                        <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">TeamSpeak non rilevato — richiesto per giocare</span>
                      )}
                      {steamRunning && teamspeakRunning && !discordRunning && serverStatus === "online" && (
                        <span className="text-mono text-[8px] text-blood-500/60 uppercase tracking-wider">Discord non rilevato — richiesto per giocare</span>
                      )}
                      {steamRunning && teamspeakRunning && discordRunning && serverStatus === "online" && accessStatus === "allowed" && !hbStatus.authenticated && (
                        <span className="text-mono text-[8px] text-gold-400/60 uppercase tracking-wider">Autenticazione del Launcher in corso…</span>
                      )}
                    </>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>

        {/* ══ GUIDA ADDON VOCALE — passi verificati dal launcher, Gioca bloccato finché non è a posto ══ */}
        {showVoiceGuide && (
          <VoiceGuideModal
            state={voicePlugin}
            teamspeakRunning={teamspeakRunning}
            onRecheck={() => { checkVoicePlugin(); checkProcesses(); }}
            onLaunchTeamspeak={() => launch("launch_teamspeak")}
            onClose={() => setShowVoiceGuide(false)}
          />
        )}

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

        {/* ══ ESILIO MODAL — istanza separata durante un ban temporaneo ══ */}
        {showExileModal && (
          <ExileModal
            ban={banInfo}
            onConfirm={() => { setShowExileModal(false); startGame(); }}
          />
        )}

        {/* ══ BOTTOM BAR — bancone del saloon ══ */}
        <div
          className="absolute bottom-0 left-0 right-0 h-14 grid items-center z-20 px-6"
          style={{
            gridTemplateColumns: "1fr auto 1fr",
            background:
              "radial-gradient(130% 220% at 50% 0%, rgba(214,138,60,.09), transparent 52%), radial-gradient(60% 100% at 50% 100%, rgba(0,0,0,.55), transparent), #120c08",
          }}
        >
          {/* rail in ottone in alto */}
          <div className="absolute inset-x-0 top-0 h-px" style={{ background: "linear-gradient(90deg, transparent, rgba(230,164,92,.65) 18%, rgba(230,164,92,.65) 82%, transparent)" }} />
          <div className="absolute inset-x-0 top-px h-px bg-black/50" />
          {/* grana sottile per legare alle carte */}
          <div className="absolute inset-0 pointer-events-none opacity-[.12] mix-blend-overlay" style={{ backgroundImage: PAPER_NOISE }} />

          {/* LEFT — meta + status */}
          <div className="flex items-center gap-2.5 relative">
            <span className="text-mono text-[8px] uppercase tracking-[.28em] text-bone-200/45">{appVersion ? `v${appVersion}` : ""}</span>
            <span className="w-1 h-1 rotate-45 bg-gold-600/40" />
            <span className="text-serif-sc text-[11px] text-bone-200/50 tracking-wide">© 2026 BorderlineRP</span>
            {serverStatus === "online" && !initializing && (
              <>
                <span className="w-1 h-1 rotate-45 bg-gold-600/40" />
                <span className="flex items-center gap-1.5">
                  <span className="w-1.5 h-1.5 rounded-full bg-moss-500" style={{ boxShadow: "0 0 7px rgba(107,122,72,1)" }} />
                  <span className="text-mono text-[8px] text-moss-500/80 uppercase tracking-wider">Online</span>
                </span>
              </>
            )}
          </div>

          {/* CENTER — EMPORIO (targa incisa in ottone) */}
          <button
            onClick={() => openUrl("https://emporio.borderlinerp.com")}
            className="emporio-btn group relative overflow-hidden flex items-center gap-3 px-7 py-2 cursor-pointer"
            style={{
              background: "linear-gradient(180deg, rgba(214,138,60,.22), rgba(120,72,24,.12) 55%, rgba(40,24,10,.2))",
              boxShadow:
                "inset 0 1px 0 rgba(255,214,150,.35), inset 0 -1px 0 rgba(0,0,0,.5), inset 0 0 0 1px rgba(230,164,92,.4), 0 0 18px -4px rgba(214,138,60,.5)",
              animation: "emporio-glow 3.4s ease-in-out infinite",
            }}
          >
            <span className="absolute inset-0 overflow-hidden pointer-events-none">
              <span className="emporio-shine absolute inset-y-0 w-1/3"
                style={{ background: "linear-gradient(90deg,transparent,rgba(255,224,170,.3),transparent)" }} />
            </span>
            <Star size={11} style={{ color: "#efbe85" }} className="relative group-hover:rotate-[72deg] transition-transform duration-500 drop-shadow-[0_1px_1px_rgba(0,0,0,.6)]" />
            <span className="text-display text-[18px] tracking-widest relative text-gold-300 group-hover:text-gold-200 transition-colors" style={{ textShadow: "0 1px 1px rgba(0,0,0,.7)" }}>
              Emporio di Borderline
            </span>
            <span className="text-gold-400/60 group-hover:text-gold-300 group-hover:translate-x-0.5 transition-all text-[12px] relative">→</span>
          </button>

          {/* RIGHT — link + aggiorna */}
          <div className="flex items-center justify-end gap-0.5 relative">
            {[
              { label: "Sito Web",    url: "https://borderlinerp.com" },
              { label: "Discord", url: "https://discord.borderlinerp.com" },
            ].map(link => (
              <button
                key={link.url}
                onClick={() => openUrl(link.url)}
                className="flex items-center gap-1.5 px-3 py-1.5 text-bone-200/60 hover:text-gold-300 transition-all cursor-pointer group"
              >
                <span className="text-serif-sc text-[12px] tracking-wide">{link.label}</span>
                <span className="text-[9px] text-bone-200/30 group-hover:text-gold-400/80 group-hover:translate-x-0.5 transition-all">→</span>
              </button>
            ))}
            <span className="w-px h-4 bg-gold-600/25 mx-2" />
            <button
              onClick={() => { fetchHealth(); checkProcesses(); resolveIdentity(); checkForUpdates(); }}
              title="Aggiorna stato + controlla aggiornamenti"
              className="text-mono text-[8px] uppercase tracking-[.28em] text-bone-200/45 hover:text-gold-300 transition-colors cursor-pointer px-2"
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
  version, notes, phase, progress, onUpdate,
}: {
  version: string; notes: string; phase: UpdatePhase;
  progress: number; onUpdate: () => void; onDismiss: () => void;
}) {
  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(4,3,2,.9)", backdropFilter: "blur(10px)" }}
    >
      <Poster width={440}>
        <div className="flex flex-col gap-4">
          {/* header */}
          <PaperHeader no="Dispaccio dalla Centrale" title="Telegramma" />
          <div className="flex items-baseline justify-center gap-2 -mt-1">
            <span className="text-serif-sc text-[24px]" style={{ color: INK.head }}>Nuova versione</span>
            <span className="text-display text-[34px] leading-none" style={{ color: INK.red }}>v{version}</span>
          </div>

          <PaperRule double />

          {/* note di rilascio */}
          {notes && (
            <p className="text-serif-sc text-[12px] leading-relaxed max-h-28 overflow-y-auto whitespace-pre-wrap" style={{ color: INK.text }}>{notes}</p>
          )}

          {/* progress */}
          {phase === "downloading" && (
            <div className="flex flex-col gap-2">
              <div className="h-1 overflow-hidden" style={{ background: "rgba(60,38,14,.25)" }}>
                <div className="h-full transition-all duration-200" style={{ width: `${progress}%`, background: INK.red }} />
              </div>
              <span className="text-mono text-[8px] uppercase tracking-wider" style={{ color: INK.soft }}>Recapito in corso… {progress}%</span>
            </div>
          )}

          {phase === "done" && (
            <span className="text-serif-sc text-[12px] text-center" style={{ color: INK.green }}>Consegnato — si riparte tra un istante…</span>
          )}

          {/* azioni */}
          {phase === "idle" && (
            <div className="flex items-center gap-3 pt-1">
              <button
                onClick={onUpdate}
                className="py-3 text-display text-[20px] tracking-[.18em] uppercase cursor-pointer transition-colors w-full"
                style={{ color: "#f3e2bd", background: INK.red, boxShadow: "0 3px 0 rgba(60,18,8,.5)" }}
              >
                Aggiorna Ora
              </button>
            </div>
          )}
        </div>
      </Poster>
    </div>
  );
}

// ── ExileModal — cosa comporta un ban temporaneo, prima dell'avvio ──────────

function ExileModal({ ban, onConfirm }: { ban: BanInfo | null; onConfirm: () => void }) {
  const expires = (() => {
    if (!ban?.expires_at) return null;
    const d = new Date(ban.expires_at);
    return isNaN(d.getTime())
      ? null
      : d.toLocaleString("it-IT", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" });
  })();

  const points = [
    "Puoi fare tutto quello che faresti normalmente: missioni, lavori, commercio, proprietà.",
    "Vedi e interagisci soltanto con altri giocatori in esilio, fino allo scadere del ban.",
    "Alla scadenza il server ti riporta da solo nell'istanza generale: nessun relog necessario.",
    "Tutti i progressi ottenuti prima del ban restano tuoi — inventario, carri, denaro, animali — e tutto ciò che fai nell'istanza separata viene mantenuto quando l'esilio finisce.",
  ];

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(4,3,2,.9)", backdropFilter: "blur(10px)" }}
    >
      <Poster width={520}>
        <div className="flex flex-col gap-4">
          <PaperHeader no="Ordinanza dello Sceriffo" title="Esilio Temporaneo" />
          <h2 className="text-display text-[26px] leading-none text-center" style={{ color: INK.head }}>
            Entrerai in un'istanza separata
          </h2>

          <PaperRule double />

          <p className="text-serif-sc text-[12px] leading-relaxed text-center" style={{ color: INK.soft }}>
            Borderline non ti chiude fuori dal territorio. Con un ban temporaneo continui a giocare
            sul server, ma in un'<span style={{ color: INK.text }}>istanza separata</span> dagli altri
            pionieri.
          </p>

          <div className="flex flex-col gap-1.5">
            {points.map((t, i) => (
              <div
                key={i}
                className="flex items-start gap-2 px-3 py-2"
                style={{ background: "rgba(60,38,14,.12)", boxShadow: "inset 0 0 0 1px rgba(230,164,92,.18)" }}
              >
                <Star size={11} style={{ color: INK.red }} className="flex-shrink-0 mt-0.5" />
                <span className="text-serif-sc text-[12px] leading-snug" style={{ color: INK.text }}>{t}</span>
              </div>
            ))}
          </div>

          {expires && (
            <p className="text-mono text-[8px] uppercase tracking-[.18em] text-center" style={{ color: INK.red }}>
              L'esilio termina il {expires}
            </p>
          )}

          <button
            onClick={onConfirm}
            className="py-3 text-display text-[18px] tracking-[.18em] uppercase cursor-pointer transition-colors w-full"
            style={{ color: "#f3e2bd", background: INK.red, boxShadow: "0 3px 0 rgba(60,18,8,.5)" }}
          >
            Ho capito
          </button>
        </div>
      </Poster>
    </div>
  );
}

// ── SplashScreen ──────────────────────────────────────────────────────────────

function SplashScreen({ exiting }: { exiting: boolean }) {
  return (
    <div
      className="absolute inset-0 z-[100] flex items-center justify-center pointer-events-none"
      style={{
        background: "#060402",
        transition: "opacity 0.6s ease",
        opacity: exiting ? 0 : 1,
      }}
    >
      <img
        src="https://cdn.borderlinerp.com/f/logo_a_1K-mosqd6j9xt8cd5.png"
        alt="BorderlineRP"
        className="h-44 w-auto object-contain"
        style={{
          filter: "drop-shadow(0 0 56px rgba(214,138,60,.65))",
          animation: "float 7s ease-in-out infinite",
        }}
        onError={e => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
      />
    </div>
  );
}

// ── CloseModal ────────────────────────────────────────────────────────────────

function CloseModal({
  keepInBackground,
  onToggle,
  onConfirm,
  onCancel,
}: {
  keepInBackground: boolean;
  onToggle: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(4,3,2,.9)", backdropFilter: "blur(10px)" }}
    >
      <Poster width={420}>
        <div className="flex flex-col gap-4">
          <PaperHeader no="Decisione del Pioniere" title="Avviso" />
          <h2 className="text-display text-[30px] leading-none text-center" style={{ color: INK.head }}>Lasci la città?</h2>

          <PaperRule double />

          <p className="text-serif-sc text-[12px] leading-relaxed text-center" style={{ color: INK.text }}>
            {keepInBackground ? "Il launcher resterà di vedetta in background." : "Chiudendolo del tutto non potrai rientrare finché non lo riapri."}
          </p>

          <div className="flex items-center justify-between py-1">
            <span className="text-serif-sc text-[12px] tracking-wide" style={{ color: INK.text }}>Resta di vedetta</span>
            <button
              onClick={onToggle}
              className="relative w-10 h-5 flex-shrink-0 transition-colors cursor-pointer"
              style={{ background: keepInBackground ? INK.green : "rgba(60,38,14,.3)", borderRadius: 9999 }}
            >
              <span
                className="absolute top-0.5 w-4 h-4 rounded-full transition-all duration-200"
                style={{ left: keepInBackground ? "calc(100% - 18px)" : 2, background: "#f3e2bd" }}
              />
            </button>
          </div>

          <div className="flex items-center gap-3 pt-1">
            <button
              onClick={onConfirm}
              className="flex-1 py-3 text-display text-[18px] uppercase tracking-[.18em] cursor-pointer transition-colors"
              style={{ color: "#f3e2bd", background: keepInBackground ? INK.head : INK.red, boxShadow: "0 3px 0 rgba(40,22,6,.5)" }}
            >
              {keepInBackground ? "Nascondi" : "Chiudi"}
            </button>
            <button
              onClick={onCancel}
              className="px-5 py-3 text-serif-sc text-[12px] tracking-wide cursor-pointer transition-opacity hover:opacity-70"
              style={{ color: INK.text, border: `1px solid ${INK.rule}` }}
            >
              Annulla
            </button>
          </div>
        </div>
      </Poster>
    </div>
  );
}

// ── HexCopy — matricola del pioniere, copiabile ──────────────────────────────

function HexCopy({ hex }: { hex: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(hex);
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    } catch { /* clipboard non disponibile */ }
  };
  return (
    <button
      onClick={copy}
      title="Copia matricola"
      className="group inline-flex items-center gap-1 mt-0.5 cursor-pointer"
    >
      <span className="text-mono text-[8px] tracking-wide" style={{ color: copied ? INK.green : INK.soft }}>
        {copied ? "copiato!" : `HEX ${hex.slice(0, 6)}…${hex.slice(-4)}`}
      </span>
      {copied
        ? <Check size={9} style={{ color: INK.green }} />
        : <Copy size={9} style={{ color: INK.faint }} className="group-hover:opacity-100 opacity-60 transition-opacity" />
      }
    </button>
  );
}

// ── Ts3Copy — indirizzo del server vocale, un tocco per copiarlo ────────────

/** Riga "Plugin vocale" nella checklist: stato dell'auto-update di Borderline Voice. */
function VoicePluginRow({ state, retry }: { state: VoicePlugin; retry: () => void }) {
  const good = state.status === "ok" || state.status === "installed";
  const text =
    state.status === "checking"       ? "…" :
    state.status === "none"           ? "– non pubblicato" :
    state.status === "conflict"       ? `✗ ${(state.conflicts ?? []).join("/") || "plugin"} in conflitto` :
    state.status === "restart_needed" ? "✗ riavvia TeamSpeak" :
    state.status === "error"          ? "✗ non aggiornato" :
    `✓ v${state.version}`;
  return (
    <div className="flex items-center justify-between">
      <span className="text-serif-sc text-[12px] tracking-wide" style={{ color: INK.text }}>Plugin vocale</span>
      {good
        ? <span className="text-display text-[13px]" style={{ color: INK.green, transform: "rotate(-5deg)", display: "inline-block" }}>{text}</span>
        : <button onClick={retry} title={state.message ?? "Apri la guida per sistemare l'addon vocale"}
            className="text-display text-[13px] cursor-pointer hover:opacity-70 transition-opacity" style={{ color: state.status === "checking" || state.status === "none" ? INK.soft : INK.red, transform: "rotate(-5deg)" }}>{text}</button>
      }
    </div>
  );
}

function Ts3Copy() {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(TS3_ADDRESS);
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    } catch { /* clipboard non disponibile */ }
  };
  return (
    <button
      onClick={copy}
      title="Copia l'indirizzo del server TeamSpeak"
      className="group flex items-center justify-center gap-1 mt-1 w-full py-1 cursor-pointer transition-all hover:brightness-[.97]"
      style={{
        // velatura d'inchiostro che sfuma ai bordi, come le regole di pagina
        background: copied
          ? "linear-gradient(90deg, transparent, rgba(74,90,20,.20) 18%, rgba(74,90,20,.20) 82%, transparent)"
          : "linear-gradient(90deg, transparent, rgba(90,58,20,.16) 18%, rgba(90,58,20,.16) 82%, transparent)",
      }}
    >
      <span className="text-mono text-[7px] uppercase tracking-[.2em]" style={{ color: INK.faint }}>TS3</span>
      <span
        className="text-serif-sc text-[11px] tracking-wide"
        style={{
          color: copied ? INK.green : INK.head,
          textDecoration: "underline", textDecorationStyle: "dotted", textUnderlineOffset: 3,
        }}
      >
        {copied ? "indirizzo copiato!" : TS3_ADDRESS}
      </span>
      {copied
        ? <Check size={9} style={{ color: INK.green }} />
        : <Copy size={9} style={{ color: INK.faint }} className="group-hover:opacity-100 opacity-60 transition-opacity" />
      }
    </button>
  );
}

// ── AccessChip — verdetto d'accesso, in inchiostro sulla carta ────────────────

function InkLink({ children }: { children: React.ReactNode }) {
  return (
    <button
      onClick={() => openUrl("https://discord.borderlinerp.com")}
      className="text-serif-sc text-[11px] tracking-wide cursor-pointer hover:opacity-70 transition-opacity"
      style={{ color: INK.head, textDecoration: "underline", textDecorationStyle: "dotted", textUnderlineOffset: 3 }}
    >
      {children}
    </button>
  );
}

function AccessChip({ status, ban }: { status: AccessStatus; ban: BanInfo | null }) {
  if (status === "loading")
    return (
      <div className="flex items-center justify-center gap-2 py-0.5">
        <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: INK.soft }} />
        <span className="text-serif-sc text-[11px] tracking-wide" style={{ color: INK.soft }}>Verifica del mandato…</span>
      </div>
    );

  if (status === "allowed")
    return (
      <div className="flex items-center gap-2">
        <Star size={20} style={{ color: INK.green }} className="drop-shadow-[0_1px_1px_rgba(255,240,200,.5)]" />
        <div>
          <div className="text-mono text-[6.5px] uppercase tracking-[.32em]" style={{ color: INK.faint }}>Verdetto</div>
          <InkStamp color={INK.green} size={16} rotate={-4}>AMMESSO</InkStamp>
        </div>
      </div>
    );

  if (status === "not_allowlisted")
    return (
      <div className="flex flex-col gap-1.5 items-center text-center">
        <InkStamp color={INK.red} size={18} rotate={-5}>NON IN LISTA</InkStamp>
        <p className="text-serif-sc text-[10.5px] leading-snug" style={{ color: INK.text }}>Richiedi l'accesso su Discord.</p>
        <InkLink>→ Apri un ticket su Discord</InkLink>
      </div>
    );

  if (status === "banned") {
    const isPerm = ban?.ban_type === "permanent";
    const expires = ban?.expires_at
      ? new Date(ban.expires_at).toLocaleDateString("it-IT", { day: "2-digit", month: "2-digit", year: "2-digit" })
      : null;
    return (
      <div className="flex flex-col gap-1.5 items-center text-center">
        <InkStamp color={INK.red} size={17} rotate={-6}>{isPerm ? "BANDITO" : "ESILIATO"}</InkStamp>
        {ban?.reason && <p className="text-serif-sc text-[10.5px] leading-snug" style={{ color: INK.text }}>{ban.reason}</p>}
        {expires && <p className="text-mono text-[7px] uppercase tracking-wider" style={{ color: INK.red }}>Revoca il {expires}</p>}
        <InkLink>→ Ricorso su Discord</InkLink>
      </div>
    );
  }

  if (status === "unknown")
    return (
      <div className="flex items-center justify-center gap-2 py-0.5">
        <span className="w-1.5 h-1.5 rounded-full" style={{ background: INK.soft }} />
        <span className="text-serif-sc text-[11px] tracking-wide" style={{ color: INK.soft }}>Steam non in linea</span>
      </div>
    );

  return (
    <div className="flex items-center justify-center gap-2 py-0.5">
      <span className="w-1.5 h-1.5 rounded-full" style={{ background: INK.soft }} />
      <span className="text-serif-sc text-[11px] tracking-wide" style={{ color: INK.soft }}>Mandato sconosciuto</span>
    </div>
  );
}
