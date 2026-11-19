import { Icon } from "./components/Icon";

export function StatusBar({ last, logError, logOpen, onLog, onTerminal }: { last: string; logError: string; logOpen: boolean; onLog: () => void; onTerminal: () => void }) {
  return <footer className="statusbar" aria-label="Status bar">
    <span className="status-message"><i className="dot tone-safe" aria-hidden="true"/><span className="status-text" title={logError || last} role={logError ? "alert" : undefined}>{logError || last}</span></span>
    <span className="status-actions">
      <button type="button" onClick={onTerminal} aria-label="Open script preview" title="Open script preview"><Icon name="terminal" size={14}/> <span>Script preview</span></button>
      <button type="button" aria-pressed={logOpen} onClick={onLog} aria-label="Toggle activity log" aria-keyshortcuts="Control+`" title="Show or hide the activity log (Ctrl+`)"><Icon name="list" size={14}/> Log</button>
    </span>
  </footer>;
}
