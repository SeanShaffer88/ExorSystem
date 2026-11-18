import { useEffect, useState } from "react";
import { api } from "../api";
import type { RiskRow, RiskView as Risk } from "../types";

const tiers: { key: keyof Risk; title: string; advice: string }[] = [
  { key: "not_assessed", title: "Not assessed", advice: "There is not enough completed, comparable evidence to rate these sessions." },
  { key: "low", title: "Lower observed risk", advice: "Few risk indicators in the available evidence. Skipped or unrequested checks are not proof that an update is safe." },
  { key: "medium", title: "Medium risk", advice: "Look at each one first." },
  { key: "high", title: "High risk", advice: "Change one at a time, after making a restore point." },
];

export function RiskView({ visible }: { visible: boolean }) {
  const [risk, setRisk] = useState<Risk | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!visible) return;
    let live = true;
    setError("");
    api.updateRisk().then((value) => { if (live) setRisk(value); }).catch((e: unknown) => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, [visible]);
  if (error) return <p role="alert" className="error">{error}</p>;
  if (!risk) return <p>Loading…</p>;
  const total = risk.low.length + risk.medium.length + risk.high.length + risk.not_assessed.length;
  return (
    <section aria-labelledby="h-risk">
      <h2 id="h-risk">Update risk</h2>
      <p>Advisory only. Based on the available evidence from the latest 50 recorded installations; not an update authorization.</p>
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
