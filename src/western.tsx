import type { CSSProperties, ReactNode } from "react";

// ════════════════════════════════════════════════════════════════════════════
//  PRIMITIVI DIEGETICI WESTERN — superfici di carta, metallo, inchiostro.
//  Palette inchiostro: testo #2a1c0e · titoli #5a3a14 · rosso #8a3018 · verde #4a5a14
// ════════════════════════════════════════════════════════════════════════════

export const INK = {
  text: "#2a1c0e",
  soft: "rgba(60,38,14,.62)",
  faint: "rgba(60,38,14,.4)",
  head: "#5a3a14",
  red: "#8a3018",
  green: "#4a5a14",
  rule: "rgba(60,38,14,.35)",
} as const;

// (esportato anche per uso esterno, es. grana della bottom bar)
export const PAPER_NOISE =
  "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='220' height='220'><filter id='p'><feTurbulence type='fractalNoise' baseFrequency='0.7' numOctaves='3' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 0.42  0 0 0 0 0.32  0 0 0 0 0.16  0 0 0 0.6 0'/></filter><rect width='100%' height='100%' filter='url(%23p)' opacity='0.5'/></svg>\")";

const PAPER_BG =
  "radial-gradient(120% 130% at 50% 0%, #e6d4ad 0%, #d8c197 45%, #c9ad7e 100%)";

// ── chiodo / puntina ─────────────────────────────────────────────────────────

export function Nail({ className = "" }: { className?: string }) {
  return (
    <span
      className={`block w-3.5 h-3.5 rounded-full ${className}`}
      style={{
        background: "radial-gradient(circle at 35% 30%, #6b5535, #2a1f12 70%)",
        boxShadow: "0 2px 4px rgba(0,0,0,.7), inset 0 1px 1px rgba(255,220,160,.4)",
      }}
    />
  );
}

// ── stella da sceriffo ────────────────────────────────────────────────────────

export function Star({ size = 16, className = "", style }: { size?: number; className?: string; style?: CSSProperties }) {
  return (
    <span
      className={className}
      style={{
        display: "inline-block",
        width: size,
        height: size,
        background: "currentColor",
        clipPath:
          "polygon(50% 0%, 61% 35%, 98% 35%, 68% 57%, 79% 91%, 50% 70%, 21% 91%, 32% 57%, 2% 35%, 39% 35%)",
        ...style,
      }}
    />
  );
}

// ── timbro d'inchiostro ────────────────────────────────────────────────────────

export function InkStamp({
  children, color = INK.red, size = 26, rotate = -7, animate = true, className = "",
}: { children: ReactNode; color?: string; size?: number; rotate?: number; animate?: boolean; className?: string }) {
  return (
    <span
      className={`text-display ${className}`}
      style={{
        color,
        fontSize: size,
        letterSpacing: ".04em",
        display: "inline-block",
        transform: `rotate(${rotate}deg)`,
        textShadow: `0 0 1px ${color}55`,
        animation: animate ? "ink-stamp .8s cubic-bezier(.2,.9,.3,1.4) both" : undefined,
        ["--stamp-rot" as string]: `${rotate}deg`,
      }}
    >
      {children}
    </span>
  );
}

// ── divisore ───────────────────────────────────────────────────────────────────

export function PaperRule({ double = false, className = "" }: { double?: boolean; className?: string }) {
  return (
    <div className={`relative ${className}`}>
      <div className="h-px w-full" style={{ background: INK.rule }} />
      {double && <div className="h-px w-full mt-[2px]" style={{ background: "rgba(60,38,14,.18)" }} />}
    </div>
  );
}

// ── intestazione "telegramma" (№ · territorio · titolo) ──────────────────────────

export function PaperHeader({ no, title }: { no?: string; title: string }) {
  return (
    <div className="text-center">
      {no && (
        <div className="text-mono text-[7px] tracking-[.36em] uppercase" style={{ color: INK.soft }}>
          {no}
        </div>
      )}
      <div className="flex items-center justify-center gap-2 mt-1">
        <span className="h-px w-6" style={{ background: INK.faint }} />
        <span className="text-serif-sc text-[11px] tracking-[.3em] uppercase" style={{ color: INK.head }}>{title}</span>
        <span className="h-px w-6" style={{ background: INK.faint }} />
      </div>
    </div>
  );
}

// ── manifesto / avviso su carta ───────────────────────────────────────────────

export function Poster({
  children, width, pinned = false, sway = false, className = "", style, padded = true,
}: {
  children: ReactNode;
  width?: number;
  pinned?: boolean;
  sway?: boolean;
  className?: string;
  style?: CSSProperties;
  padded?: boolean;
}) {
  return (
    <div
      className="relative"
      style={sway ? { animation: "paper-sway 9s ease-in-out infinite", transformOrigin: "50% -8px" } : undefined}
    >
      {pinned && (
        <Nail className="absolute left-1/2 -translate-x-1/2 -top-2 z-20" />
      )}
      <div
        className={`relative western-border ${padded ? "px-5 py-5" : ""} ${className}`}
        style={{
          width,
          background: PAPER_BG,
          color: INK.text,
          boxShadow: "0 22px 48px -18px rgba(0,0,0,.82)",
          ...style,
        }}
      >
        {/* texture carta */}
        <div className="absolute inset-0 pointer-events-none mix-blend-multiply" style={{ backgroundImage: PAPER_NOISE, opacity: 0.5 }} />
        {/* bruciature ai bordi */}
        <div className="absolute inset-0 pointer-events-none" style={{ boxShadow: "inset 0 0 24px rgba(70,40,12,.5), inset 0 0 66px rgba(40,22,6,.32)" }} />
        <div className="relative z-10">{children}</div>
      </div>
    </div>
  );
}
