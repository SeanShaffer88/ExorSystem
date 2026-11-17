import { useEffect, useState } from "react";
import { api } from "../api";
import type { Session } from "../types";

export function SessionsView() {
  const [rows, setRows] = useState<Session[] | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    api.listSessions().then(setRows).catch((e: unknown) => setError(String(e)));
  }, []);

  if (error) return <p role="alert" className="error">Could not load installations: {error}</p>;
  if (rows === null) return <p>Loading installations…</p>;
  if (rows.length === 0) return <p>No installations have been recorded yet.</p>;
  return (
    <section aria-labelledby="h-sessions">
      <h2 id="h-sessions">Installations</h2>
      <div className="table-wrap">
        <table>
          <caption className="sr-only">Recorded installation sessions, newest first</caption>
          <thead>
            <tr>
              <th scope="col" title="Session number">ID</th>
              <th scope="col" title="Application name">Application</th>
              <th scope="col" title="Detected installer technology">Installer</th>
              <th scope="col" title="Started (UTC)">Started</th>
              <th scope="col" title="Monitoring, completed or failed">Status</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((s) => (
              <tr key={s.session_id}>
                <td>{s.session_id}</td>
                <td>{s.app_name}</td>
                <td>{s.fingerprint_family ?? "unknown"}</td>
                <td>{s.started_at ?? "—"}</td>
                <td>{s.status ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
