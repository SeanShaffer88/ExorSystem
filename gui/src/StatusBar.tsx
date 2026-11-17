import { useEffect, useState } from "react";
import { logApi } from "./logApi";

export interface Shortcut {
  id: string;
  label: string;
  hint: string;
  onClick: () => void;
}

/** Bottom bar: latest activity on the left, shortcut and tool buttons on the right. */
export function StatusBar({ shortcuts }: { shortcuts: Shortcut[] }) {
  const [last, setLast] = useState("Ready.");
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let live = true;
    const a = logApi.onLine((l) => live && setLast(l.msg));
    const b = logApi.onOpen((o) => live && setOpen(o));
    logApi.isOpen().then((o) => live && setOpen(o)).catch(() => {});
    logApi.get().then((l) => live && l.length && setLast(l[l.length - 1]!.msg)).catch(() => {});
    return () => {
      live = false;
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "`") {
        e.preventDefault();
        void logApi.setOpen(!open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <footer className="statusbar" aria-label="Status bar">
      <span className="status-text" title={last}>{last}</span>
      <span className="status-actions">
        {shortcuts.map((s) => (
          <button key={s.id} type="button" onClick={s.onClick} title={s.hint}>{s.label}</button>
        ))}
        <button type="button" aria-pressed={open} onClick={() => void logApi.setOpen(!open)} title="Show or hide the activity log (Ctrl+`)">
          Log
        </button>
      </span>
    </footer>
  );
}
