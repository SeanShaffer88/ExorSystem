import { useEffect, useState } from "react";
import { api } from "../api";
import type { HistoryPage, Session } from "../types";

const PAGE_SIZE = 100;

const words: Record<string, string> = {
  created: "Added",
  modified: "Changed",
  deleted: "Removed",
  quarantined: "Moved to quarantine",
  restored: "Restored",
  renamed: "Renamed",
};

const sources: Record<string, string> = {
  snapshot_diff: "Before/after snapshots",
  usn_journal: "File journal",
  registry_callback: "Registry callback",
  manifest: "Installer manifest",
  policy: "Cleanup policy",
};

type Result = { key: string; page: HistoryPage } | { key: string; error: string };

export function HistoryView({ sessionId, onSessionChange }: {
  sessionId: number | null;
  onSessionChange: (id: number | null) => void;
}) {
  const [filter, setFilter] = useState("");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionError, setSessionError] = useState("");
  const [cursors, setCursors] = useState<(number | null)[]>([null]);
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<Result | null>(null);
  const beforeId = cursors.at(-1) ?? null;
  const path = filter.trim() || null;
  const key = JSON.stringify([sessionId, path, beforeId, attempt]);
  // Never render a previous filter or page while waiting for the new request.
  const current = result?.key === key ? result : null;
  const page = current && "page" in current ? current.page : null;
  const error = current && "error" in current ? current.error : null;
  const next = page?.next_before_id ?? null;

  useEffect(() => {
    let live = true;
    api.listSessions().then((rows) => {
      if (live) setSessions(rows);
    }).catch((e: unknown) => {
      if (live) setSessionError(`Could not load installation names: ${String(e)}`);
    });
    return () => { live = false; };
  }, []);

  useEffect(() => {
    let live = true;
    const t = window.setTimeout(() => {
      api.history(path, PAGE_SIZE, sessionId, beforeId).then((page) => {
        if (live) setResult({ key, page });
      }).catch((e: unknown) => {
        if (live) setResult({ key, error: String(e) });
      });
    }, 200);
    return () => { live = false; window.clearTimeout(t); };
  }, [key, path, sessionId, beforeId]);

  return (
    <section aria-labelledby="h-hist">
      <h2 id="h-hist">History</h2>
      <p>Recorded changes, newest first. A snapshot shows what changed between two checks, not which process changed it.</p>
      <div className="row history-filters">
        <label htmlFor="hist-session">Installation</label>
        <select id="hist-session" value={sessionId ?? ""} onChange={(e) => {
          onSessionChange(e.target.value === "" ? null : Number(e.target.value));
          setCursors([null]);
        }}>
          <option value="">All installations and unassigned changes</option>
          {sessionId !== null && !sessions.some((s) => s.session_id === sessionId) && (
            <option value={sessionId}>Session #{sessionId}</option>
          )}
          {sessions.map((s) => <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>)}
        </select>
        <label htmlFor="hist-filter">Filter by path</label>
        <input id="hist-filter" type="search" value={filter} onChange={(e) => {
          setFilter(e.target.value);
          setCursors([null]);
        }} />
      </div>
      {sessionError && <p role="alert" className="error">{sessionError}</p>}
      {error !== null ? (
        <div>
          <p role="alert" className="error">Could not load changes: {error}</p>
          <button type="button" onClick={() => setAttempt((n) => n + 1)}>Retry</button>
        </div>
      ) : (
        <p role="status">{!page ? "Loading changes…" : page.rows.length > 0
          ? `Page ${cursors.length}: ${page.rows.length} recorded change${page.rows.length === 1 ? "" : "s"}.`
          : (path || sessionId !== null) ? "No recorded changes match these filters." : "Nothing recorded yet."}</p>
      )}
      {page && page.rows.length > 0 && (
        <div className="table-wrap">
          <table>
            <caption className="sr-only">Recorded changes</caption>
            <thead><tr><th scope="col">When (UTC)</th><th scope="col">Change</th><th scope="col">Path</th><th scope="col">Source</th><th scope="col">Session</th></tr></thead>
            <tbody>
              {page.rows.map((r) => (
                <tr key={r.id}>
                  <td>{r.when ?? "Unknown"}</td><td>{words[r.what] ?? r.what}</td><td className="history-path">{r.path}</td>
                  <td>{sources[r.source] ?? r.source}</td><td>{r.session_id === null ? "Unassigned" : `#${r.session_id}`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="row">
        <button type="button" disabled={cursors.length === 1} onClick={() => setCursors((stack) => stack.slice(0, -1))}>Newer changes</button>
        <button type="button" disabled={next === null} onClick={() => {
          if (next !== null) setCursors((stack) => [...stack, next]);
        }}>Older changes</button>
      </div>
    </section>
  );
}
