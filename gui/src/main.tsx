import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";
import { initTheme } from "./theme";
import { LogWindow } from "./LogWindow";

initTheme();
if (window.location.hash === "#log") document.body.classList.add("logpage");

const el = document.getElementById("root");
if (el) {
  createRoot(el).render(
    <React.StrictMode>
      {window.location.hash === "#log" ? <LogWindow /> : <App />}
    </React.StrictMode>,
  );
}
