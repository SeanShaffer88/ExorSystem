import { useEffect, useState } from "react";
import { api } from "../api";
import { BrowsePath } from "../components/BrowsePath";
import type { PortableProgram } from "../types";

export function PortablePrograms() {
  const [entries, setEntries] = useState<PortableProgram[] | null>(null);
  const [name, setName] = useState("");
  const [executable, setExecutable] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [note, setNote] = useState("");

  useEffect(() => {
    let live = true;
    api.listPortablePrograms().then((items) => live && setEntries(items)).catch((e: unknown) => live && setError(String(e)));
    return () => { live = false; };
  }, []);

  const update = async (fn: () => Promise<void>, message: string) => {
    setBusy(true);
    setNote("");
    setError("");
    try {
      await fn();
      setNote(message);
      setEntries(await api.listPortablePrograms());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-labelledby="portable-apps">
      <h3 id="portable-apps">Portable apps</h3>
      <p>Add a program that doesn't appear in Windows' list. ExorSystem only lists the entry; it won't run it or change its files.</p>
      <form className="stack" onSubmit={(e) => {
        e.preventDefault();
        void update(async () => {
          await api.registerPortableProgram(name, executable);
          setName("");
          setExecutable("");
        }, "Portable app saved. It appears under Installations.");
      }}>
        <label htmlFor="portable-name">Program name</label>
        <input id="portable-name" value={name} maxLength={120} onChange={(e) => setName(e.target.value)} required disabled={busy} />
        <label htmlFor="portable-exe">Full path to the .exe file</label>
        <input id="portable-exe" value={executable} onChange={(e) => setExecutable(e.target.value)} required disabled={busy} />
        <BrowsePath directory={false} label="Browse executable…" onPick={setExecutable} disabled={busy}/>
        <button type="submit" disabled={busy || !name.trim() || !executable.trim()}>Save portable app</button>
      </form>
      {error && <p role="alert" className="error">{error}</p>}
      {note && <p role="status">{note}</p>}
      {entries === null ? (!error && <p>Loading portable apps…</p>) : entries.length === 0 ? (
        <p>No portable apps added.</p>
      ) : (
        <ul className="portable-list">
          {entries.map((entry) => (
            <li key={entry.executable}>
              <div><strong>{entry.name}</strong><br /><code>{entry.executable}</code></div>
              <button type="button" disabled={busy} aria-label={`Forget ${entry.name}`} onClick={() => {
                void update(() => api.forgetPortableProgram(entry.executable), "Inventory entry forgotten. No files were changed.");
              }}>Forget</button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
