import { policies, usePolicy, type Policy } from "../settings";
export function PolicyChoice({ value, onChange, disabled }: { value: Policy | null; onChange: (value: Policy | null) => void; disabled?: boolean }) {
  const current = usePolicy();
  return <label className="field">Policy<select value={value ?? ""} disabled={disabled} onChange={(e) => onChange(policies.find((p) => p === e.target.value) ?? null)}>
    <option value="">Use Settings default ({current})</option>
    {policies.map((p) => <option key={p} value={p}>{p[0]!.toUpperCase() + p.slice(1)} override</option>)}
  </select></label>;
}
