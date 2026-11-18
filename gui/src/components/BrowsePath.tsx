import { useState } from "react";
import { api } from "../api";

/** A native picker is optional; every use still exposes manual path entry. */
export function BrowsePath({ directory, label, onPick, disabled = false }: { directory: boolean; label: string; onPick: (value: string) => void; disabled?: boolean }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  return <>
    <button type="button" className="quiet-button" disabled={disabled || busy} onClick={() => {
      setBusy(true); setError("");
      void api.pickPath(directory).then((p) => { if (p !== null) onPick(p); })
        .catch((e: unknown) => setError(`Picker unavailable: ${String(e)} Enter the path manually.`)).finally(() => setBusy(false));
    }}>{busy ? "Choosing…" : label}</button>
    {error && <small role="alert" className="error">{error}</small>}
  </>;
}
