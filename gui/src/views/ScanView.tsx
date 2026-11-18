import { useState } from "react";
import { api } from "../api";
import { BrowsePath } from "../components/BrowsePath";
import type { ScanEvent, ScanProgress } from "../types";

const mb = (b: number) => (b / (1024 * 1024)).toFixed(1);

export function ScanView() {
  const [root, setRoot] = useState("");
  const [strict, setStrict] = useState(false);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [outcome, setOutcome] = useState("");
  const [error, setError] = useState("");

  const onEvent = (e: ScanEvent) => {
    switch (e.event) {
      case "progress":
        setProgress(e.data);
        break;
      case "done":
        setOutcome(`Scan finished. ${e.data.files} files found; ${e.data.skipped} items could not be fully checked. Fingerprint ${e.data.root_digest.slice(0, 16)}.`);
        break;
      case "cancelled":
        setOutcome("Scan cancelled. Nothing was changed.");
        break;
      case "failed":
        setError(e.data.message);
        break;
    }
  };

  const start = async (ev: React.FormEvent) => {
    ev.preventDefault();
    if (!root.trim() || running) return;
    setRunning(true);
    setError("");
    setOutcome("");
    setProgress(null);
    try {
      await api.scan(root.trim(), strict, onEvent);
    } catch (err) {
      setError(String(err));
    } finally {
      setRunning(false);
    }
  };

  const phase = progress?.phase === "walking" ? "Looking through folders…" : progress?.phase === "hashing" ? "Reading files…" : "";
  return (
    <section aria-labelledby="h-scan">
      <h2 id="h-scan">Scan folder</h2>
      <p>Records the current state of a folder. Read only; the window stays responsive while it runs.</p>
      <form onSubmit={start}>
        <label htmlFor="scan-root">Folder</label>
        <input id="scan-root" value={root} onChange={(e) => setRoot(e.target.value)} title="Full path of the folder to scan, for example C:\Program Files\App" />
        <BrowsePath directory label="Browse scan folder…" onPick={setRoot} disabled={running}/>
        <label className="check"><input type="checkbox" checked={strict} disabled={running} onChange={(e) => setStrict(e.target.checked)}/> Preserve case-only paths</label>
        <button type="submit" disabled={running} title="Start scanning this folder">Scan</button>
        <button type="button" disabled={!running} onClick={() => void api.cancelScan().catch((e: unknown) => setError(String(e)))} title="Stop the scan; nothing is changed">Cancel</button>
      </form>
      {running && progress && (
        <div>
          <p>{phase}</p>
          <progress
            aria-label="Scan progress"
            value={progress.phase === "hashing" ? progress.files_processed : undefined}
            max={progress.phase === "hashing" ? Math.max(progress.files_seen, 1) : undefined}
          />
          <p>
            {progress.files_processed} of {progress.files_seen || "…"} files processed · {progress.files_hashed} hashed · {progress.files_reused} reused · {progress.files_unhashed} unhashed · {mb(progress.bytes_hashed)} MiB read
          </p>
        </div>
      )}
      <div role="status" aria-live="polite">{outcome}</div>
      {error && <p role="alert" className="error">{error}</p>}
    </section>
  );
}
