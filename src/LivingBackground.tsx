import { useEffect, useRef, useState } from "react";

// ════════════════════════════════════════════════════════════════════════════
//  SFONDO VIVO — scena fotografica + braci, polvere, fumo, raggio, parallax.
//  Rimpiazza lo sfondo statico. Si auto-traccia il mouse per il parallax.
// ════════════════════════════════════════════════════════════════════════════

const BANNER = "https://cdn.borderlinerp.com/f/banner-mosqd6hsqkzpdm.png";

const LB_KEYFRAMES = `
  @keyframes lb-drift-a { 0%{transform:translate(0,0)} 50%{transform:translate(38px,-20px)} 100%{transform:translate(0,0)} }
  @keyframes lb-drift-b { 0%{transform:translate(0,0)} 50%{transform:translate(-34px,16px)} 100%{transform:translate(0,0)} }
  @keyframes lb-ray     { 0%,100%{opacity:.08;transform:translateX(0) skewX(-12deg)} 50%{opacity:.22;transform:translateX(26px) skewX(-12deg)} }
`;

interface Mote {
  x: number; y: number; r: number; vy: number; vx: number;
  life: number; max: number; ember: boolean;
}

interface Props {
  /** numero di particelle (braci+polvere). Default 90. Usa ~45 per "più sobrio". */
  density?: number;
  /** quanto scurire il centro (per far risaltare l'hero). 0–1. Default .55 */
  centerDark?: number;
}

export default function LivingBackground({ density = 90, centerDark = 0.55 }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [par, setPar] = useState({ x: 0, y: 0 });
  const raf = useRef(0);

  // parallax sul movimento del mouse
  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      cancelAnimationFrame(raf.current);
      raf.current = requestAnimationFrame(() => {
        setPar({
          x: e.clientX / window.innerWidth - 0.5,
          y: e.clientY / window.innerHeight - 0.5,
        });
      });
    };
    window.addEventListener("mousemove", onMove);
    return () => { window.removeEventListener("mousemove", onMove); cancelAnimationFrame(raf.current); };
  }, []);

  // braci + polvere su canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const W = (canvas.width = 1100);
    const H = (canvas.height = 660);

    const motes: Mote[] = [];
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
    for (let i = 0; i < density; i++) motes.push(spawn(true));

    let id = 0;
    let t = 0;
    const tick = () => {
      t += 0.016;
      ctx.clearRect(0, 0, W, H);
      for (const m of motes) {
        m.life += 1;
        m.y -= m.vy;
        m.x += m.vx + Math.sin((m.y + t * 30) * 0.01) * 0.3;
        if (m.y < -10 || m.life > m.max) Object.assign(m, spawn());
        const fade = Math.min(1, m.life / 40) * (1 - m.life / m.max);
        if (m.ember) {
          const flick = 0.7 + Math.sin(t * 6 + m.x) * 0.3;
          ctx.beginPath();
          ctx.arc(m.x, m.y, m.r, 0, Math.PI * 2);
          ctx.fillStyle = `rgba(${(230 + Math.random() * 25) | 0}, ${(140 + Math.sin(t + m.x) * 30) | 0}, 60, ${fade * flick * 0.9})`;
          ctx.shadowBlur = 8;
          ctx.shadowColor = "rgba(230,140,60,.7)";
          ctx.fill();
        } else {
          ctx.beginPath();
          ctx.arc(m.x, m.y, m.r, 0, Math.PI * 2);
          ctx.fillStyle = `rgba(220,205,170,${fade * 0.28})`;
          ctx.shadowBlur = 0;
          ctx.fill();
        }
      }
      ctx.shadowBlur = 0;
      id = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(id);
  }, [density]);

  return (
    <div className="absolute inset-0 overflow-hidden pointer-events-none" style={{ background: "#0c0805" }}>
      <style>{LB_KEYFRAMES}</style>

      {/* scena con lento zoom + parallax */}
      <img
        src={BANNER}
        alt=""
        aria-hidden
        className="absolute inset-0 w-full h-full object-cover infinite-zoom"
        onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
        style={{
          opacity: 0.52,
          filter: "sepia(.2) brightness(.66) contrast(1.06)",
          transform: `scale(1.08) translate(${par.x * -14}px, ${par.y * -14}px)`,
          transition: "transform .25s ease-out",
        }}
      />

      {/* raggio di luce diagonale */}
      <div
        className="absolute -top-1/4 left-1/3 w-40 h-[150%]"
        style={{
          background: "linear-gradient(90deg, transparent, rgba(230,164,92,.12), transparent)",
          filter: "blur(22px)",
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
          transform: `translate(${par.x * -24}px, ${par.y * -18}px)`,
        }}
      />
      <div
        className="absolute"
        style={{
          inset: "-10%",
          background: "radial-gradient(520px 300px at 60% 92%, rgba(20,14,10,.7), transparent 72%)",
          animation: "lb-drift-b 34s ease-in-out infinite",
          transform: `translate(${par.x * -8}px, ${par.y * -6}px)`,
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
