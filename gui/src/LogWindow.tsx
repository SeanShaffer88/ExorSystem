import { useEffect, useState } from "react";
import { LogLines, useActivityLog } from "./components/ActivityLog";
import { logApi } from "./logApi";
import { initTheme } from "./theme";

/** The owned native window attached below the desktop workspace. */
export function LogWindow() {
  const log = useActivityLog();
  const [error, setError] = useState("");
  useEffect(() => {
    const onStorage = (e: StorageEvent) => { if (e.key === null || e.key === "exor.theme") initTheme(); };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);
  const close = () => {
    setError("");
    void logApi.setOpen(false).catch((e: unknown) => setError(`Could not close the activity log: ${String(e)}`));
  };
  return <div className="logwin" onKeyDown={(e) => {
    if (e.key === "Escape" || (e.ctrlKey && !e.altKey && e.key === "`")) {
      e.preventDefault();
      if (!e.repeat) close();
    }
  }}>
    <div className="logbar">
      <strong>Activity log</strong>
      <span>
        <button type="button" onClick={() => void log.clear()} disabled={!log.lines.length || log.clearing}>Clear</button>{" "}
        <button type="button" onClick={close}>Close</button>
      </span>
    </div>
    {(log.error || error) && <p role="alert" className="error activity-error">{log.error || error}</p>}
    <LogLines lines={log.lines}/>
  </div>;
}
