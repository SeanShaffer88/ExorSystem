import { useRef, useState } from "react";
import { api } from "../api";
import type { Usage } from "../types";
import { BrowsePath } from "./BrowsePath";
import { formatBytes } from "./Workspace";

const categories = [
  ["database_bytes", "SQLite evidence"], ["wal_bytes", "WAL and related files"],
  ["manifests_bytes", "Quarantine manifests"], ["held_bytes", "Held file bytes"],
  ["migration_backup_bytes", "Pre-migration database copies"], ["other_bytes", "Other profile files"],
] as const;

export function ProfileStorage() {
  const [usage, setUsage] = useState<Usage | null>(null);
  const [destination, setDestination] = useState("");
  const [backup, setBackup] = useState("");
  const [working, setWorking] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const running = useRef(false);
  const work = async (fn: () => Promise<void>) => {
    if (running.current) return;
    running.current = true; setWorking(true); setNote(""); setError("");
    try { await fn(); } catch (e) { setError(String(e)); }
    finally { running.current = false; setWorking(false); }
  };
  return <section aria-labelledby="profile-storage">
    <h3 id="profile-storage">Profile storage and backup</h3>
    <p>Measurements are logical lengths of files in this profile, not allocated or available disk space. Exports and backups outside this profile are not included.</p>
    <button type="button" disabled={working} onClick={() => void work(async () => setUsage(await api.storageUsage()))}>Measure profile storage</button>
    {usage && <><dl className="facts">{categories.map(([key, label]) => <div key={key}><dt>{label}</dt><dd>{formatBytes(usage[key])}</dd></div>)}</dl>
      {usage.incomplete && <p role="status">Partial measurement: some paths were skipped or exceeded the limit.</p>}</>}
    <div className="stack">
      <p>Back up SQLite and verified held files to a new folder. This does not delete local evidence. A restore verifies the backup and writes a separate copy; it never replaces the active profile.</p>
      <label className="field">Destination parent<input value={destination} maxLength={4096} onChange={(e) => setDestination(e.target.value)} disabled={working}/></label>
      <BrowsePath directory label="Browse destination…" onPick={setDestination} disabled={working}/>
      <button className="quiet-button" type="button" disabled={working || !destination.trim()} onClick={() => void work(async () => {
        const result = await api.backupProfile(destination);
        setBackup(result.path); setNote(`Verified ${result.files} file(s), ${formatBytes(result.bytes)}. Saved to ${result.path}.`);
      })}>Create verified backup</button>
      <label className="field">Existing backup directory<input value={backup} maxLength={4096} onChange={(e) => setBackup(e.target.value)} disabled={working}/></label>
      <BrowsePath directory label="Browse existing backup…" onPick={setBackup} disabled={working}/>
      <button className="quiet-button" type="button" disabled={working || !destination.trim() || !backup.trim()} onClick={() => void work(async () => {
        const result = await api.restoreProfileCopy(backup, destination);
        setNote(`Verified a separate copy at ${result.path}. The active profile was not changed.`);
      })}>Restore to separate copy</button>
    </div>
    {working && <p role="status">Working on the profile copy or measurement. Keep the app open until verification finishes.</p>}
    {note && <p role="status">{note}</p>}
    {error && <p role="alert" className="error">{error}</p>}
  </section>;
}
