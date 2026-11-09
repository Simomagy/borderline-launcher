import { useEffect, useRef } from "react";

// ════════════════════════════════════════════════════════════════════════════
//  SFONDO VIVO — scena fotografica + braci, polvere, fumo, raggio.
//  Rimpiazza lo sfondo statico.
// ════════════════════════════════════════════════════════════════════════════

const BANNER = "https://cdn.borderlinerp.com/f/banner-mosqd6hsqkzpdm.png";

const LB_KEYFRAMES = `
  @keyframes lb-drift-a { 0%{transform:translate(0,0)} 50%{transform:translate(38px,-20px)} 100%{transform:translate(0,0)} }
  @keyframes lb-drift-b { 0%{transform:translate(0,0)} 50%{transform:translate(-34px,16px)} 100%{transform:translate(0,0)} }
  @keyframes lb-ray     { 0%,100%{opacity:.08;transform:translateX(0) skewX(-12deg)} 50%{opacity:.22;transform:translateX(26px) skewX(-12deg)} }
`;

// Le braci si ridisegnano a ~30 fps a prescindere dal refresh del monitor
// (su un 144/240 Hz il vecchio loop a rAF girava 4 volte più spesso).
const FRAME_MS = 33;

interface Mote {
  x: number; y: number; r: number; vy: number; vx: number;
  life: number; max: number; ember: boolean;
}

interface Props {
  /** numero di particelle (braci+polvere). Default 90. Usa ~45 per "più sobrio". */
  density?: number;
  /** quanto scurire il centro (per far risaltare l'hero). 0–1. Default .55 */
  centerDark?: number;
  /** finestra non attiva: il loop delle braci si ferma (le animazioni CSS le ferma App). */
  paused?: boolean;
}

/** Bagliore della brace pre-renderizzato una volta: sostituisce `shadowBlur`, che
 *  costava un gaussian blur per ogni brace a ogni frame. */
function emberSprite(): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = c.height = 32;
  const g = c.getContext("2d")!;
  const grad = g.createRadialGradient(16, 16, 0, 16, 16, 16);
  grad.addColorStop(0, "rgba(255,200,120,1)");
  grad.addColorStop(0.14, "rgba(240,150,70,.9)");
  grad.addColorStop(0.4, "rgba(230,140,60,.25)");
  grad.addColorStop(1, "rgba(230,140,60,0)");
  g.fillStyle = grad;
  g.fillRect(0, 0, 32, 32);
  return c;
}

export default function LivingBackground({ density = 90, centerDark = 0.55, paused = false }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pausedRef = useRef(paused);
  const wake = useRef<(() => void) | null>(null);

  useEffect(() => {
    pausedRef.current = paused;
    if (!paused) wake.current?.();
  }, [paused]);

  // braci + polvere su canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const W = (canvas.width = 1100);
    const H = (canvas.height = 660);
    const sprite = emberSprite();

    const spawn = (initial = false): Mote => {
      const ember = Math.random() > 0.45;
      return {
        x: Math.random() * W,
        y: initial ? Math.random() * H : H + 10,
        r: ember ? 0.8 + Math.random() * 1.6 : 0.4 + Math.random() * 0.9,
        vy: ember ? 0.25 + Math.random() * 0.55 : 0.08 + Math.random() * 0.22,
        vx: (Math.random() - 0.5) * 0.4,
        life: 0,
        max: 220 + Math.random() * 320,
        ember,
      };
    };
    const motes: Mote[] = Array.from({ length: density }, () => spawn(true));

    let id = 0;
    let t = 0;
    let last = 0;
    const tick = () => {
      if (pausedRef.current) { id = 0; return; }
      const now = performance.now();
      // dt in "frame a 60 fps": stesse velocità di prima, indipendenti dal frame rate
      const dt = Math.min((now - last) / 16.67, 4);
      last = now;
      t += 0.016 * dt;
      ctx.clearRect(0, 0, W, H);
      for (const m of motes) {
        m.life += dt;
        m.y -= m.vy * dt;
        m.x += (m.vx + Math.sin((m.y + t * 30) * 0.01) * 0.3) * dt;
        if (m.y < -10 || m.life > m.max) Object.assign(m, spawn());
        const fade = Math.max(0, Math.min(1, m.life / 40) * (1 - m.life / m.max));
        if (m.ember) {
          const flick = 0.7 + Math.sin(t * 6 + m.x) * 0.3;
          const s = 14 + m.r * 6;
          ctx.globalAlpha = fade * flick * 0.9;
          ctx.drawImage(sprite, m.x - s / 2, m.y - s / 2, s, s);
        } else {
          ctx.globalAlpha = fade * 0.28;
          ctx.fillStyle = "rgb(220,205,170)";
          ctx.beginPath();
          ctx.arc(m.x, m.y, m.r, 0, Math.PI * 2);
          ctx.fill();
        }
      }
      ctx.globalAlpha = 1;
      id = window.setTimeout(tick, FRAME_MS);
    };
    wake.current = () => { if (!id) { last = performance.now(); tick(); } };
    wake.current();
    return () => { clearTimeout(id); wake.current = null; };
  }, [density]);

  return (
    <div className="absolute inset-0 overflow-hidden pointer-events-none" style={{ background: "#0c0805" }}>
      <style>{LB_KEYFRAMES}</style>

      {/* scena con lento zoom */}
      <img
        src={BANNER}
        alt=""
        aria-hidden
        className="absolute inset-0 w-full h-full object-cover infinite-zoom"
        onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
        style={{ opacity: 0.52, filter: "sepia(.2) brightness(.66) contrast(1.06)" }}
      />

      {/* raggio di luce diagonale (il gradiente è già sfumato: niente filter blur per frame) */}
      <div
        className="absolute -top-1/4 left-1/3 w-40 h-[150%]"
        style={{
          background: "linear-gradient(90deg, transparent, rgba(230,164,92,.12), transparent)",
          animation: "lb-ray 11s ease-in-out infinite",
        }}
      />

      {/* fumo / nebbia che deriva */}
      <div
        className="absolute"
        style={{
          inset: "-10%",
          background:
            "radial-gradient(420px 280px at 22% 80%, rgba(40,28,18,.6), transparent 70%), radial-gradient(360px 240px at 80% 28%, rgba(30,20,14,.5), transparent 70%)",
          animation: "lb-drift-a 26s ease-in-out infinite",
        }}
      />
      <div
        className="absolute"
        style={{
          inset: "-10%",
          background: "radial-gradient(520px 300px at 60% 92%, rgba(20,14,10,.7), transparent 72%)",
          animation: "lb-drift-b 34s ease-in-out infinite",
        }}
      />

      {/* braci + polvere */}
      <canvas ref={canvasRef} className="absolute inset-0 w-full h-full" />

      {/* vignette + scurimento centrale (mute dell'emblema dietro l'hero) */}
      <div
        className="absolute inset-0"
        style={{
          background: `linear-gradient(to bottom, rgba(8,5,3,.72) 0%, transparent 24%, transparent 52%, rgba(8,5,3,.97) 100%), radial-gradient(ellipse 70% 60% at 50% 46%, rgba(8,5,3,${centerDark}) 0%, transparent 60%)`,
        }}
      />
    </div>
  );
}
