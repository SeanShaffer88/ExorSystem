import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { QuarantineRow, Session } from "../types";

const statusText: Record<string, string> = {
  quarantined: "In quarantine",
  restored: "Restored",
  purged: "Deleted permanently",
  rolled_back: "Undone after interruption",
  in_progress: "Interrupted",
};

export function QuarantineView() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [template, setTemplate] = useState("standard");
  const [rows, setRows] = useState<QuarantineRow[]>([]);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [confirmMove, setConfirmMove] = useState(false);
  const [confirmPurge, setConfirmPurge] = useState<string | null>(null);

  const refresh = useCallback(() => api.listQuarantines().then(setRows).catch((e: unknown) => setError(String(e))), []);

  useEffect(() => {
    api.listSessions().then((s) => {
      setSessions(s);
      setSessionId(s[0]?.session_id ?? null);
    }).catch((e: unknown) => setError(String(e)));
    void refresh();
  }, [refresh]);

  const act = async (fn: () => Promise<string>) => {
    setError("");
    try {
      setMessage(await fn());
    } catch (e) {
      setError(String(e));
    }
    setConfirmMove(false);
    setConfirmPurge(null);
    await refresh();
  };

  return (
    <section aria-labelledby="h-q">
      <h2 id="h-q">Quarantine</h2>
      <p>Files are moved aside, not deleted. You can put them back until you delete them permanently.</p>

      <form onSubmit={(e) => e.preventDefault()}>
        <label htmlFor="q-session">Installation</label>
        <select id="q-session" value={sessionId ?? ""} onChange={(e) => setSessionId(Number(e.target.value))} title="Installation to clean up">
          {sessions.map((s) => (<option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>))}
        </select>
        <label htmlFor="q-tpl">Policy</label>
        <select id="q-tpl" value={template} onChange={(e) => setTemplate(e.target.value)} title="How cautious the cleanup rules are">
          <option value="conservative">Conservative</option>
          <option value="standard">Standard</option>
          <option value="thorough">Thorough</option>
        </select>
        {!confirmMove ? (
          <button type="button" disabled={sessionId === null} onClick={() => setConfirmMove(true)} title="Move the files this policy selects into quarantine">
            Move to quarantine…
          </button>
        ) : (
          <span role="group" aria-label="Confirm move to quarantine">
            <button type="button" onClick={() => sessionId !== null && act(async () => (await api.quarantineSession(sessionId, template)).summary)}>
              Confirm: move files
            </button>
            <button type="button" onClick={() => setConfirmMove(false)}>Cancel</button>
          </span>
        )}
      </form>

      {error && <p role="alert" className="error">{error}</p>}
      {message && <p role="status" aria-live="polite">{message}</p>}

      <div className="table-wrap">
        <table>
          <caption className="sr-only">Quarantine batches</caption>
          <thead>
            <tr>
              <th scope="col">Batch</th>
              <th scope="col">Installation</th>
              <th scope="col">Files</th>
              <th scope="col">State</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 && (<tr><td colSpan={5}>Nothing has been quarantined yet.</td></tr>)}
            {rows.map((r) => (
              <tr key={r.quarantine_id}>
                <td>{r.quarantine_id}</td>
                <td>{r.session_id ?? "—"}</td>
                <td>{r.files}</td>
                <td>{statusText[r.status] ?? r.status}</td>
                <td>
                  {r.status === "quarantined" && confirmPurge !== r.quarantine_id && (
                    <>
                      <button type="button" onClick={() => act(async () => `Restored ${await api.restoreQuarantine(r.quarantine_id)} item(s).`)} title="Put the files back where they were">
                        Restore
                      </button>{" "}
                      <button type="button" onClick={() => setConfirmPurge(r.quarantine_id)} title="Delete these files permanently">
                        Delete permanently…
                      </button>
                    </>
                  )}
                  {confirmPurge === r.quarantine_id && (
                    <span role="group" aria-label="Confirm permanent delete">
                      This cannot be undone.{" "}
                      <button type="button" onClick={() => act(async () => `Deleted ${await api.purgeQuarantine(r.quarantine_id)} item(s) permanently.`)}>
                        Confirm: delete
                      </button>{" "}
                      <button type="button" onClick={() => setConfirmPurge(null)}>Cancel</button>
                    </span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
