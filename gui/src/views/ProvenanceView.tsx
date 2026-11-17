import { useState } from "react";
import { api } from "../api";
import type { ProvenanceView as P } from "../types";

export function ProvenanceView() {
  const [path, setPath] = useState("");
  const [result, setResult] = useState<P | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!path.trim()) return;
    setBusy(true);
    setError("");
    try {
      setResult(await api.provenance(path.trim()));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-labelledby="h-prov">
      <h2 id="h-prov">Provenance</h2>
      <form onSubmit={submit}>
        <label htmlFor="path">File path or registry key</label>
        <input id="path" value={path} onChange={(e) => setPath(e.target.value)} title="Enter the full path to trace; press Enter to search" />
        <button type="submit" disabled={busy} title="Trace this item back to the installation that created it">Trace</button>
      </form>
      {error && <p role="alert" className="error">{error}</p>}
      {result && (
        <div role="status" aria-live="polite">
          <p>{result.summary}</p>
          {result.found && (
            <div className="table-wrap">
              <table>
                <caption className="sr-only">Change history</caption>
                <thead>
                  <tr>
                    <th scope="col">When (UTC)</th>
                    <th scope="col">Change</th>
                    <th scope="col">Seen by</th>
                  </tr>
                </thead>
                <tbody>
                  {result.ledger.map((l) => (
                    <tr key={l.entry_id}>
                      <td>{l.observed_at ?? "—"}</td>
                      <td>{l.entry.change_kind}</td>
                      <td>{l.entry.source}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
