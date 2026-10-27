import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import App from "./App";

const root = document.getElementById("root") as HTMLElement;

// Il layout è fisso a 1100×680, ma WebView2 ingrandisce il contenuto con la
// "Dimensione testo" di Windows (Accessibilità) o tra monitor con DPI diversi,
// mentre la finestra resta uguale: destra e basso venivano tagliati. Adattiamo.
const fit = () => { root.style.zoom = String(Math.min(innerWidth / 1100, innerHeight / 680)); };
fit();
addEventListener("resize", fit);

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
