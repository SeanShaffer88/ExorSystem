import { useEffect, useState } from "react";
import { logApi } from "./logApi";
import { Icon } from "./components/Icon";

export function StatusBar({ onTerminal }: { onTerminal: () => void }) {
  const [last, setLast] = useState("Ready");
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    const a = logApi.onLine((l) => { if (live) setLast(l.msg); });
    const b = logApi.onOpen((o) => { if (live) setOpen(o); });
    // Attach rejection handlers immediately; a plain browser may not have Tauri IPC.
    const listeners = [a, b].map((p) => p.catch(() => () => {}));
    logApi.isOpen().then((o) => { if (live) setOpen(o); }).catch(() => {});
    logApi.get().then((l) => { if (live && l.length) setLast(l[l.length - 1]!.msg); }).catch(() => {});
    return () => { live = false; for (const listener of listeners) void listener.then((stop) => stop()).catch(() => {}); };
  }, []);

  const toggleLog = () => {
    setError("");
    void logApi.setOpen(!open).catch((e: unknown) => setError(String(e)));
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!document.querySelector("dialog[open]") && e.ctrlKey && e.key === "`") {
        e.preventDefault();
        void logApi.setOpen(!open).catch((err: unknown) => setError(String(err)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  return <footer className="statusbar" aria-label="Status bar">
    <span className="status-message"><i className="dot tone-safe" aria-hidden="true"/><span className="status-text" title={error || last}>{error || last}</span></span>
    <span className="status-actions">
      <button type="button" onClick={onTerminal} title="Open script preview"><Icon name="terminal" size={14}/> <span>Script preview</span></button>
      <button type="button" aria-pressed={open} onClick={toggleLog} title="Show or hide the activity log (Ctrl+`)"><Icon name="list" size={14}/> Log</button>
    </span>
  </footer>;
}
