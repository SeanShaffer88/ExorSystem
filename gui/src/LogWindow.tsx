import { useCallback, useEffect, useState } from "react";
import { LogLines, useActivityLog } from "./components/ActivityLog";
import { logApi } from "./logApi";
import { initTheme } from "./theme";

/** The owned native window attached below the desktop workspace. */
export function LogWindow() {
  const log = useActivityLog();
  const [error, setError] = useState("");
  const close = useCallback(() => {
    setError("");
    void logApi.setOpen(false).catch((e: unknown) => setError(`Could not close the activity log: ${String(e)}`));
  }, []);
  useEffect(() => {
    const onStorage = (e: StorageEvent) => { if (e.key === null || e.key === "exor.theme") initTheme(); };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || (e.ctrlKey && !e.altKey && e.key === "`")) {
        e.preventDefault();
        if (!e.repeat) close();
      }
    };
    window.addEventListener("storage", onStorage);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("storage", onStorage);
      window.removeEventListener("keydown", onKey);
    };
  }, [close]);
  return <div className="logwin">
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
