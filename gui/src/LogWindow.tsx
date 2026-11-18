import { useEffect, useState } from "react";
import { LogLines, useActivityLog } from "./components/ActivityLog";
import { logApi } from "./logApi";
import { initTheme } from "./theme";

/** The optional native window that follows the desktop at its right edge. */
export function LogWindow() {
  const log = useActivityLog();
  const [error, setError] = useState("");
  useEffect(() => {
    const onStorage = (e: StorageEvent) => { if (e.key === null || e.key === "exor.theme") initTheme(); };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);
  return <div className="logwin">
    <div className="logbar">
      <strong>Activity log</strong>
      <span>
        <button type="button" onClick={() => void log.clear()} disabled={!log.lines.length || log.clearing}>Clear</button>{" "}
        <button type="button" onClick={() => void logApi.setOpen(false).catch((e: unknown) => setError(String(e)))}>Hide</button>
      </span>
    </div>
    {(log.error || error) && <p role="alert" className="error activity-error">{log.error || error}</p>}
    <LogLines lines={log.lines}/>
  </div>;
}
