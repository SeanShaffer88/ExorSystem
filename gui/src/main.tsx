import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";
import "./dashboard.css";
import { initTheme } from "./theme";
import { LogWindow } from "./LogWindow";

initTheme();
if (window.location.hash === "#log") document.body.classList.add("logpage");

function render(demo = false) {
  const el = document.getElementById("root");
  if (el) {
    createRoot(el).render(
      <React.StrictMode>
        {window.location.hash === "#log" ? <LogWindow /> : <App demo={demo} />}
      </React.StrictMode>,
    );
  }
}

// Browser-only synthetic data stays out of production and never replaces native Tauri IPC.
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  void import("./demo").then(({ startDemo }) => { startDemo(); render(true); });
} else {
  render();
}
