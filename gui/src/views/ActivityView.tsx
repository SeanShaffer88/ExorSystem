import { useEffect, useState } from "react";
import { isDemo } from "../bridge";
import { LogLines, type ActivityLogState } from "../components/ActivityLog";
import { Icon } from "../components/Icon";
import { logApi } from "../logApi";

export function ActivityView({ log, visible }: { log: ActivityLogState; visible: boolean }) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  const demo = isDemo();
  useEffect(() => {
    if (demo) return;
    let live = true;
    const listener = logApi.onOpen((o) => { if (live) setOpen(o); }).catch(() => () => {});
    logApi.isOpen().then((o) => { if (live) setOpen(o); }).catch(() => {});
    return () => { live = false; void listener.then((stop) => stop()).catch(() => {}); };
  }, [demo]);
  const toggle = () => {
    setError("");
    void logApi.setOpen(!open).catch((e: unknown) => setError(`Could not open or hide the attached window: ${String(e)}`));
  };
  return <section className="activity-panel" aria-labelledby="activity-title">
    <header className="activity-tools">
      <div><h1 id="activity-title">Activity log</h1><small>Local events · this session</small></div>
      <div className="button-row">
        {!demo && <button className="quiet-button" type="button" aria-pressed={open} onClick={toggle}><Icon name="dockRight" size={15}/>{open ? "Hide attached window" : "Open attached window"}</button>}
        <button className="quiet-button" type="button" onClick={() => void log.clear()} disabled={!log.lines.length || log.clearing}>Clear log</button>
      </div>
    </header>
    {(log.error || error) && <p role="alert" className="error activity-error">{log.error || error}</p>}
    <LogLines lines={log.lines} visible={visible}/>
  </section>;
}
