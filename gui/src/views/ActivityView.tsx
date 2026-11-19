import { LogLines, type ActivityLogState } from "../components/ActivityLog";

export function ActivityView({ log, visible }: { log: ActivityLogState; visible: boolean }) {
  return <section className="activity-panel" aria-labelledby="activity-title">
    <header className="activity-tools">
      <div><h1 id="activity-title">Activity log</h1><small>Local events · this session</small></div>
      <div className="button-row">
        <button className="quiet-button" type="button" onClick={() => void log.clear()} disabled={!log.lines.length || log.clearing}>Clear log</button>
      </div>
    </header>
    {log.error && <p role="alert" className="error activity-error">{log.error}</p>}
    <LogLines lines={log.lines} visible={visible}/>
  </section>;
}
