import { useEffect } from "react";
import { Poster, PaperHeader, PaperRule, INK } from "./western";

/** Stato dell'addon vocale come lo ritorna il comando Rust `ensure_voice_plugin`. */
export type VoicePlugin = {
  status: "checking" | "ok" | "installed" | "restart_needed" | "conflict" | "none" | "error";
  version?: string;
  message?: string;
  conflicts?: string[];
  /** true = Borderline Voice è partito in TS3, false = un altro plugin ha la porta, null = non verificabile. */
  plugin_active?: boolean | null;
  teamspeak_running?: boolean;
};

/** Stati che bloccano il tasto Gioca finché la guida non è completata. */
export const voiceBlocked = (s: VoicePlugin["status"]) => s === "conflict" || s === "restart_needed" || s === "error";

type Step = { title: string; hint?: string; done: boolean; action?: { label: string; run: () => void } };

/**
 * Guida passo-passo per sistemare l'addon vocale. Ogni passo è verificato dal
 * launcher: la guida ripete il controllo ogni 2 s e spunta da sola i passi fatti.
 *   - conflict:       disattiva YaCA/SaltyChat → riavvia TS3 → verifica dal log di TeamSpeak
 *   - restart_needed: chiudi TS3 → (installazione automatica) → riapri TS3
 *   - error:          messaggio + riprova
 *
 * Lo stato "disattivato" di un plugin altrui non è leggibile dall'esterno: il passo si
 * spunta quando Borderline Voice riesce finalmente a partire, che è la cosa che conta.
 */
export function VoiceGuideModal({
  state, teamspeakRunning, onRecheck, onLaunchTeamspeak, onClose,
}: {
  state: VoicePlugin; teamspeakRunning: boolean;
  onRecheck: () => void; onLaunchTeamspeak: () => void; onClose: () => void;
}) {
  useEffect(() => {
    onRecheck();
    const t = setInterval(onRecheck, 2000);
    return () => clearInterval(t);
  }, [onRecheck]);

  const tsRunning = state.teamspeak_running ?? teamspeakRunning;
  const conflicts = state.conflicts ?? [];
  const pluginActive = state.plugin_active ?? null;
  const installed = state.status === "ok" || state.status === "installed";

  const steps: Step[] = [];
  if (pluginActive === false) {
    steps.push({
      title: conflicts.length > 0
        ? `Disattiva ${conflicts.join(" e ")} in TeamSpeak`
        : "Disattiva gli altri plugin vocali in TeamSpeak",
      hint: "Tools › Options › Addons: porta l'interruttore del plugin su Disabled. Non premere Uninstall, perderesti impostazioni e licenza.",
      done: false,
      action: tsRunning ? undefined : { label: "Apri TeamSpeak", run: onLaunchTeamspeak },
    });
  }
  steps.push({
    title: "Chiudi TeamSpeak",
    hint: "Chiudilo del tutto, anche dall'icona vicino all'orologio: finché è aperto la DLL del plugin non si può sostituire.",
    done: !tsRunning || (installed && pluginActive !== false),
  });
  steps.push({
    title: "Installazione di Borderline Voice",
    hint: "Automatica: parte appena TeamSpeak è chiuso.",
    done: installed,
  });
  steps.push({
    title: "Riapri TeamSpeak",
    hint: "Al riavvio TeamSpeak carica il plugin aggiornato e io controllo dal log che sia partito.",
    done: installed && tsRunning && pluginActive !== false,
    action: installed && !tsRunning ? { label: "Apri TeamSpeak", run: onLaunchTeamspeak } : undefined,
  });

  const allDone = steps.every(s => s.done);
  const current = steps.findIndex(s => !s.done);

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center" style={{ background: "rgba(4,3,2,.9)", backdropFilter: "blur(10px)" }}>
      <Poster width={470}>
        <div className="flex flex-col gap-4">
          <PaperHeader no="Ufficio del telegrafo" title="Addon vocale" />
          <p className="text-serif-sc text-[12px] text-center leading-snug" style={{ color: INK.text }}>
            {state.status === "error"
              ? "Non riesco ad aggiornare l'addon vocale."
              : "Per giocare serve l'addon vocale Borderline Voice in TeamSpeak. Segui i passi: li verifico io."}
          </p>
          <PaperRule double />

          {state.status === "error" ? (
            <p className="text-serif-sc text-[12px] leading-relaxed whitespace-pre-wrap" style={{ color: INK.red }}>{state.message ?? "Errore sconosciuto"}</p>
          ) : (
            <ol className="flex flex-col gap-2.5">
              {steps.map((s, i) => {
                const isCurrent = i === current;
                const color = s.done ? INK.green : isCurrent ? INK.text : INK.faint;
                return (
                  <li key={s.title} className="flex gap-3">
                    <span className="text-display text-[18px] leading-none w-5 shrink-0" style={{ color: s.done ? INK.green : isCurrent ? INK.red : INK.faint }}>
                      {s.done ? "✓" : i + 1}
                    </span>
                    <div className="flex flex-col gap-0.5 min-w-0">
                      <span className="text-serif-sc text-[13px] tracking-wide" style={{ color, textDecoration: s.done ? "line-through" : "none" }}>{s.title}</span>
                      {isCurrent && s.hint && <span className="text-serif-sc text-[11px] leading-snug" style={{ color: INK.soft }}>{s.hint}</span>}
                      {isCurrent && s.action && (
                        <button onClick={s.action.run} className="self-start mt-1 px-3 py-1 text-display text-[13px] uppercase tracking-[.1em] cursor-pointer"
                          style={{ color: "#f3e2bd", background: INK.red, boxShadow: "0 2px 0 rgba(60,18,8,.5)" }}>{s.action.label}</button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ol>
          )}

          <PaperRule />
          <div className="flex items-center justify-between gap-3">
            <span className="text-mono text-[8px] uppercase tracking-wider" style={{ color: INK.soft }}>
              {allDone ? "Tutto a posto" : "Controllo ogni 2 secondi…"}
            </span>
            <div className="flex gap-2">
              {state.status === "error" && (
                <button onClick={onRecheck} className="px-3 py-1 text-display text-[13px] uppercase tracking-[.1em] cursor-pointer" style={{ color: "#f3e2bd", background: INK.red }}>Riprova</button>
              )}
              <button onClick={onClose} className="px-3 py-1 text-display text-[13px] uppercase tracking-[.1em] cursor-pointer hover:opacity-70" style={{ color: INK.head }}>
                {allDone ? "Chiudi" : "Più tardi"}
              </button>
            </div>
          </div>
        </div>
      </Poster>
    </div>
  );
}
