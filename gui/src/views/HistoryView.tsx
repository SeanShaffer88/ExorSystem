import { useEffect, useState } from "react";
import { api } from "../api";
import type { HistoryRow } from "../types";

const words: Record<string, string> = {
  created: "Added",
  modified: "Changed",
  deleted: "Removed",
  quarantined: "Moved to quarantine",
  restored: "Restored",
  renamed: "Renamed",
};

export function HistoryView() {
  const [filter, setFilter] = useState("");
  const [rows, setRows] = useState<HistoryRow[] | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let live = true;
    const t = window.setTimeout(() => {
      api.history(filter, 300).then((r) => live && setRows(r)).catch((e: unknown) => live && setError(String(e)));
    }, 200);
    return () => { live = false; window.clearTimeout(t); };
  }, [filter]);

  return (
    <section aria-labelledby="h-hist">
      <h2 id="h-hist">History</h2>
      <p>Every change ExorSystem has recorded, newest first.</p>
      <label htmlFor="hist-filter" className="sr-only">Filter by path</label>
      <input id="hist-filter" type="search" placeholder="Filter by path" value={filter} onChange={(e) => setFilter(e.target.value)} />
      {error && <p role="alert" className="error">{error}</p>}
      {rows === null ? <p>Loading…</p> : rows.length === 0 ? <p>Nothing recorded yet.</p> : (
        <div className="table-wrap">
          <table>
            <caption className="sr-only">Recorded changes</caption>
            <thead><tr><th scope="col">When</th><th scope="col">What</th><th scope="col">Path</th><th scope="col">Install</th></tr></thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.id}><td>{r.when ?? "Unknown"}</td><td>{words[r.what] ?? r.what}</td><td>{r.path}</td><td>{r.session_id === null ? "" : `#${r.session_id}`}</td></tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
