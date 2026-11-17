import { useCallback, useEffect, useState } from "react";
import { flags } from "./flags";
import { ScanView } from "./views/ScanView";
import { ProvenanceView } from "./views/ProvenanceView";
import { SessionsView } from "./views/SessionsView";
import { AppearanceView } from "./views/AppearanceView";
import { ThemeUndoBar } from "./ThemeUndoBar";
import { WatchView } from "./views/WatchView";
import { QuarantineView } from "./views/QuarantineView";
import { HistoryView } from "./views/HistoryView";
import { SimulatorView } from "./views/SimulatorView";
import { StatusBar, type Shortcut } from "./StatusBar";

type ViewId = "scan" | "sessions" | "provenance" | "simulator" | "quarantine" | "history" | "appearance" | "watch";
const views: { id: ViewId; label: string; enabled: boolean }[] = [
  { id: "sessions", label: "Installations", enabled: true },
  { id: "watch", label: "Watch an install", enabled: flags.watchView },
  { id: "scan", label: "Scan folder", enabled: true },
  { id: "provenance", label: "Provenance", enabled: true },
  { id: "simulator", label: "Cleanup simulator", enabled: true },
  { id: "quarantine", label: "Quarantine", enabled: flags.quarantineView },
  { id: "history", label: "History", enabled: true },
  { id: "appearance", label: "Appearance", enabled: flags.themes },
];

export function App() {
  const [view, setView] = useState<ViewId>("sessions");
  const [announce, setAnnounce] = useState("");
  const enabled = views.filter((v) => v.enabled);

  const go = useCallback((id: ViewId) => {
    setView(id);
    const label = views.find((v) => v.id === id)?.label ?? id;
    setAnnounce(`${label} view`);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey) return;
      const idx = Number(e.key) - 1;
      const target = enabled[idx];
      if (target) {
        e.preventDefault();
        go(target.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled, go]);

  const onTabKey = (e: React.KeyboardEvent) => {
    const i = enabled.findIndex((v) => v.id === view);
    const n = enabled.length;
    const to = e.key === "ArrowRight" ? (i + 1) % n : e.key === "ArrowLeft" ? (i + n - 1) % n : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : -1;
    const t = enabled[to];
    if (t) {
      e.preventDefault();
      go(t.id);
      document.getElementById(`tab-${t.id}`)?.focus();
    }
  };

  const shortcuts: Shortcut[] = (
    [
      ["watch", "Watch", "Record a new installation"],
      ["scan", "Scan", "Scan a folder"],
      ["quarantine", "Quarantine", "Open quarantined items"],
      ["appearance", "Themes", "Change the look"],
    ] as const
  )
    .filter(([id]) => enabled.some((v) => v.id === id))
    .map(([id, label, hint]) => ({ id, label, hint, onClick: () => go(id) }));

  return (
    <div className="shell">
      <a className="skip" href="#main">Skip to content</a>
      <header className="topbar">
        <h1>ExorSystem</h1>
        <div role="tablist" aria-label="Tools" onKeyDown={onTabKey}>
          {enabled.map((v, i) => (
            <button
              key={v.id}
              id={`tab-${v.id}`}
              type="button"
              role="tab"
              aria-selected={view === v.id}
              aria-controls="main"
              tabIndex={view === v.id ? 0 : -1}
              title={`Open ${v.label} (Alt+${i + 1})`}
              onClick={() => go(v.id)}
            >
              {v.label}
            </button>
          ))}
        </div>
        <ThemeUndoBar />
      </header>
      <main id="main" role="tabpanel" aria-labelledby={`tab-${view}`} tabIndex={-1}>
        {view === "sessions" && <SessionsView />}
        {view === "watch" && <WatchView />}
        {view === "scan" && <ScanView />}
        {view === "provenance" && <ProvenanceView />}
        {view === "simulator" && <SimulatorView />}
        {view === "quarantine" && <QuarantineView />}
        {view === "history" && <HistoryView />}
        {view === "appearance" && <AppearanceView />}
      </main>
      <StatusBar shortcuts={shortcuts} />
      <div className="sr-only" role="status" aria-live="polite">{announce}</div>
    </div>
  );
}
