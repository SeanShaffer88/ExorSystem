import { useCallback, useEffect, useState } from "react";
import { flags } from "./flags";
import { ScanView } from "./views/ScanView";
import { ProvenanceView } from "./views/ProvenanceView";
import { SessionsView } from "./views/SessionsView";
import { AppearanceView } from "./views/AppearanceView";
import { ThemeUndoBar } from "./ThemeUndoBar";
import { QuarantineView } from "./views/QuarantineView";
import { SimulatorView } from "./views/SimulatorView";

type ViewId = "scan" | "sessions" | "provenance" | "simulator" | "quarantine" | "appearance";
const views: { id: ViewId; label: string; enabled: boolean }[] = [
  { id: "sessions", label: "Installations", enabled: true },
  { id: "scan", label: "Scan folder", enabled: true },
  { id: "provenance", label: "Provenance", enabled: true },
  { id: "simulator", label: "Cleanup simulator", enabled: true },
  { id: "quarantine", label: "Quarantine", enabled: flags.quarantineView },
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

  return (
    <div className="shell">
      <a className="skip" href="#main">Skip to content</a>
      <nav aria-label="Main">
        <h1>ExorSystem</h1>
        <ul>
          {enabled.map((v, i) => (
            <li key={v.id}>
              <button
                type="button"
                aria-current={view === v.id ? "page" : undefined}
                title={`Open ${v.label} (Alt+${i + 1})`}
                onClick={() => go(v.id)}
              >
                {v.label} <kbd>Alt+{i + 1}</kbd>
              </button>
            </li>
          ))}
        </ul>
        <ThemeUndoBar />
      </nav>
      <main id="main" tabIndex={-1}>
        {view === "sessions" && <SessionsView />}
        {view === "scan" && <ScanView />}
        {view === "provenance" && <ProvenanceView />}
        {view === "simulator" && <SimulatorView />}
        {view === "quarantine" && <QuarantineView />}
        {view === "appearance" && <AppearanceView />}
      </main>
      <div className="sr-only" role="status" aria-live="polite">{announce}</div>
    </div>
  );
}
