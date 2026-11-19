import { useState } from "react";
import type { WatchSystem } from "../types";

export type WatchScopes = { folders: string; registrySubtrees: string; system: WatchSystem };
type Preset = WatchScopes & { name: string };
const key = "exor.watch.presets.v1";
const maxPresets = 12;
const maxScope = 8192;
const maxName = 48;
const hasScope = (s: WatchScopes) => !!(s.folders.trim() || s.registrySubtrees.trim() || s.system.services || s.system.tasks);

function isPreset(value: unknown): value is Preset {
  if (typeof value !== "object" || value === null) return false;
  const p = value as Partial<Preset>;
  return typeof p.name === "string" && p.name.length <= maxName && !!p.name.trim()
    && typeof p.folders === "string" && p.folders.length <= maxScope
    && typeof p.registrySubtrees === "string" && p.registrySubtrees.length <= maxScope
    && typeof p.system?.services === "boolean" && typeof p.system.tasks === "boolean";
}

function readPresets(): Preset[] {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null || raw.length > 512 * 1024) return [];
    const list: unknown = JSON.parse(raw);
    if (!Array.isArray(list)) return [];
    const seen = new Set<string>();
    return list.filter(isPreset).filter((p) => {
      const name = p.name.trim().toLowerCase();
      if (seen.has(name) || !hasScope(p)) return false;
      seen.add(name);
      return true;
    }).slice(0, maxPresets);
  } catch { return []; }
}

export function WatchPresets({ scopes, disabled, onApply }: { scopes: WatchScopes; disabled: boolean; onApply: (scopes: WatchScopes) => void }) {
  const [presets, setPresets] = useState(readPresets);
  const [selected, setSelected] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const preset = presets.find((p) => p.name === selected);
  const save = (next: Preset[], selection: string, note: string) => {
    try { localStorage.setItem(key, JSON.stringify(next)); }
    catch { setError("Could not save scope presets on this device."); setMessage(""); return false; }
    setPresets(next); setSelected(selection); setError(""); setMessage(note);
    return true;
  };
  const add = () => {
    const title = name.trim();
    if (!title || !hasScope(scopes)) return;
    setMessage("");
    if (presets.length >= maxPresets) { setError(`Remove a preset before saving more than ${maxPresets}.`); return; }
    if (presets.some((p) => p.name.trim().toLowerCase() === title.toLowerCase())) { setError("A preset already uses that name. Choose a different name."); return; }
    if (scopes.folders.length > maxScope || scopes.registrySubtrees.length > maxScope) { setError("Scope text is too long to save as a preset."); return; }
    if (save([...presets, { name: title, ...scopes }], title, `Saved scope preset ${title}.`)) setName("");
  };

  return <details className="watch-presets">
    <summary>Saved scope presets</summary>
    <p className="muted">Presets save folder and registry paths and system check choices locally, not installer paths or snapshots. Applying one fills the form without starting a watch.</p>
    <fieldset disabled={disabled}>
      <legend className="sr-only">Manage scope presets</legend>
      <div className="tool-form">
        <div className="field"><label htmlFor="scope-preset">Scope preset</label><select id="scope-preset" value={selected} onChange={(e) => setSelected(e.target.value)}>
          <option value="">Select a saved preset</option>
          {presets.map((p) => <option key={p.name} value={p.name}>{p.name}</option>)}
        </select></div>
        <button type="button" disabled={!preset} onClick={() => {
          if (!preset) return;
          onApply(preset); setError(""); setMessage(`Applied ${preset.name}. Review the scopes before starting.`);
        }}>Apply scopes</button>
        <button type="button" disabled={!preset} onClick={() => preset && save(presets.filter((p) => p !== preset), "", `Removed ${preset.name}.`)}>Forget preset</button>
      </div>
      <div className="tool-form">
        <label className="field">New preset name<input maxLength={maxName} value={name} onChange={(e) => setName(e.target.value)}/></label>
        <button type="button" disabled={!name.trim() || !hasScope(scopes)} onClick={add}>Save current scopes</button>
      </div>
    </fieldset>
    {message && <p role="status">{message}</p>}
    {error && <p className="error" role="alert">{error}</p>}
  </details>;
}
