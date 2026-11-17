import { useEffect, useState } from "react";
import { api } from "../api";
import type { RiskRow, RiskView as Risk } from "../types";

const tiers: { key: keyof Risk; title: string; advice: string }[] = [
  { key: "low", title: "Low risk", advice: "Safe to change together." },
  { key: "medium", title: "Medium risk", advice: "Look at each one first." },
  { key: "high", title: "High risk", advice: "Change one at a time, after making a restore point." },
];

export function RiskView() {
  const [risk, setRisk] = useState<Risk | null>(null);
  const [error, setError] = useState("");
  useEffect(() => { api.updateRisk().then(setRisk).catch((e: unknown) => setError(String(e))); }, []);
  if (error) return <p role="alert" className="error">{error}</p>;
  if (!risk) return <p>Loading…</p>;
  const total = risk.low.length + risk.medium.length + risk.high.length;
  return (
    <section aria-labelledby="h-risk">
      <h2 id="h-risk">Update risk</h2>
      <p>How risky it is to change each program you have recorded, based on what its installation touched.</p>
      {total === 0 && <p>No recorded installations yet. Use Watch an install first.</p>}
      {tiers.map((t) => (
        <div key={t.key}>
          <h3>{t.title} ({risk[t.key].length})</h3>
          <p>{t.advice}</p>
          <ul>{risk[t.key].map((r: RiskRow, i: number) => <li key={`${r.name}-${i}`}><strong>{r.name}</strong>. {r.why}</li>)}</ul>
        </div>
      ))}
    </section>
  );
}
