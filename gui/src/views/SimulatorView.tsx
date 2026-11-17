import { useEffect, useState } from "react";
import { api } from "../api";
import { getPolicy } from "../settings";
import type { Session, SimulationView } from "../types";

const templates = [
  { id: "conservative", label: "Conservative" },
  { id: "standard", label: "Standard" },
  { id: "thorough", label: "Thorough" },
];

const actionText = { keep: "Keep", quarantine: "Quarantine", flag_for_review: "Review" } as const;

export function SimulatorView() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [template, setTemplate] = useState(getPolicy());
  const [result, setResult] = useState<SimulationView | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    api.listSessions().then((s) => {
      setSessions(s);
      setSessionId(s[0]?.session_id ?? null);
    }).catch((e: unknown) => setError(String(e)));
  }, []);

  const run = async (e: React.FormEvent) => {
    e.preventDefault();
    if (sessionId === null) return;
    setError("");
    try {
      setResult(await api.simulate(sessionId, template));
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <section aria-labelledby="h-sim">
      <h2 id="h-sim">Cleanup simulator</h2>
      <p>Dry run only. Nothing is changed on your computer.</p>
      <form onSubmit={run}>
        <label htmlFor="session">Installation</label>
        <select id="session" value={sessionId ?? ""} onChange={(e) => setSessionId(Number(e.target.value))} title="Choose the installation to clean up">
          {sessions.map((s) => (
            <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>
          ))}
        </select>
        <label htmlFor="tpl">Policy</label>
        <select id="tpl" value={template} onChange={(e) => setTemplate(e.target.value)} title="How cautious the cleanup rules are">
          {templates.map((t) => (<option key={t.id} value={t.id}>{t.label}</option>))}
        </select>
        <button type="submit" disabled={sessionId === null} title="Evaluate the policy without changing anything">Run dry run</button>
      </form>
      {error && <p role="alert" className="error">{error}</p>}
      {result && (
        <div role="status" aria-live="polite">
          <p>{result.summary}</p>
          <div className="table-wrap">
            <table>
              <caption className="sr-only">Proposed action for each item</caption>
              <thead>
                <tr>
                  <th scope="col">Item</th>
                  <th scope="col">Action</th>
                  <th scope="col">Why</th>
                </tr>
              </thead>
              <tbody>
                {result.decisions.map((d) => (
                  <tr key={d.path}>
                    <td>{d.path}</td>
                    <td>{actionText[d.action]}</td>
                    <td>{d.reason}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </section>
  );
}
