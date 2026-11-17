import { useState } from "react";
import { api } from "../api";
import type { WatchSummary } from "../types";

type Stage = "idle" | "watching" | "done";

export function WatchView() {
  const [stage, setStage] = useState<Stage>("idle");
  const [appName, setAppName] = useState("");
  const [installer, setInstaller] = useState("");
  const [folders, setFolders] = useState("C:\\Program Files\nC:\\Program Files (x86)\nC:\\ProgramData");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [summary, setSummary] = useState<WatchSummary | null>(null);

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const begin = (e: React.FormEvent) => {
    e.preventDefault();
    void run(async () => {
      await api.beginWatch(appName, installer.trim() || null, folders.split("\n"));
      setSummary(null);
      setStage("watching");
    });
  };

  return (
    <section aria-labelledby="h-watch">
      <h2 id="h-watch">Watch an install</h2>
      <p>
        Take a picture of your system, run the installer, then take another. Every difference is recorded so
        it can be traced and cleaned up later.
      </p>

      {stage !== "watching" && (
        <form onSubmit={begin}>
          <label htmlFor="w-app">Application name</label>
          <input id="w-app" value={appName} onChange={(e) => setAppName(e.target.value)} required title="What you are about to install" />
          <label htmlFor="w-inst">Installer file (optional)</label>
          <input id="w-inst" value={installer} onChange={(e) => setInstaller(e.target.value)} title="Full path of the setup program" />
          <label htmlFor="w-folders">Folders to watch, one per line</label>
          <textarea id="w-folders" rows={4} cols={60} value={folders} onChange={(e) => setFolders(e.target.value)} title="Large folders take longer to scan" />
          <button type="submit" disabled={busy}>{busy ? "Taking the first picture…" : "Take the first picture"}</button>
        </form>
      )}

      {stage === "watching" && (
        <div role="status" aria-live="polite">
          <p>Ready. Run the installer now. When it has finished, come back and press Finish.</p>
          <div className="row">
            <button type="button" disabled={busy} onClick={() => run(async () => { setSummary(await api.finishWatch()); setStage("done"); })}>
              {busy ? "Comparing…" : "Finish: take the second picture"}
            </button>
            <button type="button" disabled={busy} onClick={() => run(async () => { await api.cancelWatch(); setStage("idle"); })}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {error && <p role="alert" className="error">{error}</p>}

      {stage === "done" && summary && (
        <div role="status" aria-live="polite">
          <p>{summary.summary}</p>
          {summary.warnings.length > 0 && (
            <>
              <h3>Not checked</h3>
              <ul>{summary.warnings.map((w) => (<li key={w}>{w}</li>))}</ul>
            </>
          )}
          <p>Open Provenance or Cleanup simulator to see what this installation left behind (session #{summary.session_id}).</p>
        </div>
      )}
    </section>
  );
}
