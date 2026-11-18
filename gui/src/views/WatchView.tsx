import { useState } from "react";
import { api } from "../api";
import type { WatchStart, WatchSummary } from "../types";

type Operation = "begin" | "finish" | "cancel";

function WatchWarnings({ warnings }: { warnings: string[] }) {
  if (!warnings.length) return null;
  return (
    <>
      <h3>Not checked</h3>
      <ul>{warnings.map((w) => <li key={w}>{w}</li>)}</ul>
    </>
  );
}

export function WatchView({ hidden, onOpenHistory }: { hidden: boolean; onOpenHistory: (sessionId: number) => void }) {
  const [appName, setAppName] = useState("");
  const [installer, setInstaller] = useState("");
  const [folders, setFolders] = useState("C:\\Program Files\nC:\\Program Files (x86)\nC:\\ProgramData");
  const [registrySubtrees, setRegistrySubtrees] = useState("");
  const [active, setActive] = useState<WatchStart | null>(null);
  const [pending, setPending] = useState<Operation | null>(null);
  const [error, setError] = useState("");
  const [summary, setSummary] = useState<WatchSummary | null>(null);

  const run = async (operation: Operation, fn: () => Promise<void>) => {
    if (pending) return;
    setPending(operation);
    setError("");
    setSummary(null);
    try {
      await fn();
    } catch (e) {
      const message = String(e);
      setError(operation === "finish" ? `Could not finish recording: ${message}. Start a new watch to try again.` : message);
    } finally {
      // Finish and Cancel consume the backend watch even when recording or closing it fails.
      if (operation !== "begin") setActive(null);
      setPending(null);
    }
  };

  const begin = (e: React.FormEvent) => {
    e.preventDefault();
    void run("begin", async () => {
      setActive(await api.beginWatch(appName, installer.trim() || null, folders.split("\n"), registrySubtrees));
    });
  };
  const busy = pending !== null;

  return (
    <section aria-labelledby="h-watch" hidden={hidden}>
      <h2 id="h-watch">Watch an install</h2>
      <p>
        Take a before picture, run the installer, then take an after picture. Changes between the two pictures
        are clues, not proof that the installer made them.
      </p>

      {!active && (
        <form className="stack" onSubmit={begin}>
          <label htmlFor="w-app">Application name</label>
          <input id="w-app" value={appName} onChange={(e) => setAppName(e.target.value)} required disabled={busy} title="What you are about to install" />
          <label htmlFor="w-inst">Installer file (optional)</label>
          <input id="w-inst" value={installer} onChange={(e) => setInstaller(e.target.value)} disabled={busy} title="Full path of the setup program" />
          <label htmlFor="w-folders">Folders to watch, one per line</label>
          <textarea id="w-folders" rows={4} cols={60} value={folders} onChange={(e) => setFolders(e.target.value)} disabled={busy} title="Large folders take longer to scan" />
          <label htmlFor="w-registry">Registry subtrees to watch (optional)</label>
          <small id="w-registry-hint">
            Enter up to eight specific Software subtrees, one per line, such as <code>HKCU\Software\Example</code> or{" "}
            <code>HKLM\Software\Example</code>. Use <code>HKLM32</code> for the 32-bit machine view.
            Leave blank to skip the registry. Only key names and last-write times are checked; values are not collected.
          </small>
          <textarea id="w-registry" rows={3} cols={60} maxLength={8192} aria-describedby="w-registry-hint" value={registrySubtrees} onChange={(e) => setRegistrySubtrees(e.target.value)} disabled={busy} />
          <button type="submit" disabled={busy}>{busy ? "Taking the first picture…" : "Take the first picture"}</button>
        </form>
      )}

      {active && (
        <div>
          <div role="status" aria-live="polite">
            <p>{busy ? (pending === "cancel" ? "Stopping the watch…" : "Taking the after picture…") :
              `Ready to watch ${appName} (session #${active.session_id}). Run the installer, then come back and press Finish. You can switch tabs while the watch is open.`}</p>
            <WatchWarnings warnings={active.warnings} />
          </div>
          <div className="row">
            <button type="button" disabled={busy} onClick={() => run("finish", async () => { setSummary(await api.finishWatch()); })}>
              Finish: take the second picture
            </button>
            <button type="button" disabled={busy} onClick={() => run("cancel", api.cancelWatch)}>Cancel</button>
          </div>
        </div>
      )}

      {error && <p role="alert" className="error">{error}</p>}

      {summary && (
        <div>
          <div role="status" aria-live="polite">
            <p>{summary.summary}</p>
            <p>{summary.registry_scopes_compared > 0
              ? `Registry key changes: ${summary.registry_keys_changed} across ${summary.registry_scopes_compared} compared subtree(s). Registry findings are for review only.`
              : "No registry subtrees were compared."}</p>
            <WatchWarnings warnings={summary.warnings} />
          </div>
          <button type="button" onClick={() => onOpenHistory(summary.session_id)}>
            View recorded changes (session #{summary.session_id})
          </button>
        </div>
      )}
    </section>
  );
}
