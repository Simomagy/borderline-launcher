import { useState } from "react";
import { motion } from "motion/react";
import buttonSvgUrl from "./assets/button.svg";
import { Play, X } from "lucide-react";

const CORNERS = [
  { id: "TL", points: "0 17 0 2.11 2.11 0 17 0 17 5.56 6.37 5.56 6.37 17 0 17",                          dx: -8, dy: -8 },
  { id: "BL", points: "17 184 2.11 184 0 181.89 0 167 5.56 167 5.56 177.63 17 177.63 17 184",            dx: -8, dy:  8 },
  { id: "BR", points: "760 167 760 181.89 757.89 184 743 184 743 178.44 753.63 178.44 753.63 167 760 167", dx:  8, dy:  8 },
  { id: "TR", points: "743 0 757.89 0 760 2.11 760 17 754.44 17 754.44 6.37 743 6.37 743 0",             dx:  8, dy: -8 },
] as const;

const CORNER_SPRING = { type: "spring" as const, stiffness: 420, damping: 30 };
const WRAPPER_SPRING = { type: "spring" as const, stiffness: 380, damping: 22, mass: 0.8 };

interface PlayButtonProps {
  canPlay: boolean;
  label: string;
  onClick: () => void;
}

export function PlayButton({ canPlay, label, onClick }: PlayButtonProps) {
  const [hovered, setHovered] = useState(false);

  return (
    <motion.div
      className="relative select-none"
      style={{ width: 380, height: 92, cursor: canPlay ? "pointer" : "not-allowed" }}
      onHoverStart={() => canPlay && setHovered(true)}
      onHoverEnd={() => setHovered(false)}
      whileHover={canPlay ? { scale: 1.055, y: -5 } : undefined}
      whileTap={canPlay ? { scale: 0.965, y: 2 } : undefined}
      transition={WRAPPER_SPRING}
      onClick={canPlay ? onClick : undefined}
    >
      {/* Glow bloom dietro il bottone */}
      <motion.div
        className="absolute inset-0 pointer-events-none"
        style={{ zIndex: -1, filter: "blur(26px)" }}
        animate={
          hovered
            ? { scale: 1.22, background: "rgba(201,161,74,.55)" }
            : { scale: 1.00, background: "rgba(201,161,74,.20)" }
        }
        transition={{ duration: 0.28, ease: "easeOut" }}
      />

      {/* Riempimento interno */}
      {/* <div
        className="absolute"
        style={{
          inset: "7px",
          background: canPlay
            ? "linear-gradient(110deg, #b45309 0%, #d97706 38%, #fbbf24 62%, #d97706 100%)"
            : "linear-gradient(to right, rgba(8,6,3,.92), rgba(20,14,6,.92))",
        }}
      /> */}

      {/* Shimmer */}
      {canPlay && (
        <div className="absolute overflow-hidden" style={{ inset: "7px" }}>
          <span
            className="absolute inset-y-0 left-0 w-1/4"
            style={{
              background: "linear-gradient(90deg,transparent,rgba(255,255,255,.22),transparent)",
              animation: "shimmer-x 2.6s linear infinite",
            }}
          />
        </div>
      )}

      {/* Bordo SVG decorativo (statico) */}
      {canPlay && (
        <img
          src={buttonSvgUrl}
          aria-hidden
          draggable={false}
          className="absolute inset-0 w-full h-full pointer-events-none"
        style={{ opacity: canPlay ? 1 : 0.32 }}
      />
      )}

      {/* Corners animati in overlay — si aprono verso l'esterno sull'hover */}
      {canPlay && (
        <svg
          viewBox="0 0 760 184"
          width={380}
          height={92}
          aria-hidden
          style={{ position: "absolute", inset: 0, pointerEvents: "none", overflow: "visible" }}
        >
          {CORNERS.map(({ id, points, dx, dy }) => (
            <motion.polygon
              key={id}
              fill="#dcb968"
              points={points}
              animate={hovered ? { x: dx, y: dy } : { x: 0, y: 0 }}
              transition={CORNER_SPRING}
            />
          ))}
        </svg>
      )}

      {/* Etichetta */}
      <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
        <span
          className={`text-display ${canPlay ? "text-[45px] gap-6" : "text-xl gap-2"} tracking-[.22em] uppercase select-none flex items-center`}
          style={{ color: canPlay ? "#c9a14a" : "#ed2939", textShadow: canPlay ? "0 0 12px rgba(10, 7, 6,.88)" : "0 0 4px rgba(10,7,6,.88)" }}
        >
          {canPlay ? <Play size={45} /> : <X size={24} />}{label}
        </span>
      </div>
    </motion.div>
  );
}
