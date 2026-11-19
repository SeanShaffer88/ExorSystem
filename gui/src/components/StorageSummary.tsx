import type { QuarantineRow } from "../types";
import { CardHeading, Empty, formatBytes } from "./Workspace";

export function summarizeStorage(rows: QuarantineRow[]) {
  const totals = { held: 0, purged: 0, restored: 0, heldBatches: 0, missing: 0 };
  for (const row of rows) {
    if (row.status === "quarantined") totals.heldBatches++;
    if (row.status !== "quarantined" && row.status !== "purged" && row.status !== "restored") continue;
    const bytes = row.file_bytes;
    if (bytes == null || !Number.isFinite(bytes) || bytes < 0) totals.missing++;
    else totals[row.status === "quarantined" ? "held" : row.status] += bytes;
  }
  return totals;
}

export function StorageSummary({ rows, action }: { rows: QuarantineRow[]; action?: React.ReactNode }) {
  const stats = summarizeStorage(rows);
  const segments = [
    { label: "Permanently deleted", value: stats.purged, tone: "safe" },
    { label: "Held in quarantine", value: stats.held, tone: "violet" },
    { label: "Returned by restore", value: stats.restored, tone: "blue" },
  ];
  const total = segments.reduce((sum, s) => sum + s.value, 0);
  let offset = 0;
  const ring = segments.map((s) => {
    const share = total ? s.value / total * 100 : 0;
    const start = offset;
    offset += share;
    return share > 0 ? <circle key={s.label} className={`storage-arc tone-${s.tone}`} cx="60" cy="60" r="49" pathLength="100" strokeDasharray={`${share} ${100 - share}`} strokeDashoffset={-start}/> : null;
  });
  return <section className="card storage-card" aria-label="Storage reclamation">
    <CardHeading title="Storage reclamation" subtitle="Recorded sizes by batch outcome · displayed batches" action={action}/>
    {rows.length ? <div className="storage-content">
      <div className="storage-ring">
        <svg aria-hidden="true" viewBox="0 0 120 120"><circle className="storage-track" cx="60" cy="60" r="49"/>{ring}</svg>
        <span><strong>{formatBytes(stats.purged)}</strong><small>Deleted</small></span>
      </div>
      <dl className="storage-legend">
        {segments.map((s) => <div key={s.label}><dt><i className={`dot tone-${s.tone}`} aria-hidden="true"/>{s.label}</dt><dd>{formatBytes(s.value)}</dd></div>)}
      </dl>
      <p className="muted storage-note">Logical file sizes, not measured free space. Files held in quarantine have not been reclaimed.
        {stats.missing > 0 && ` ${stats.missing} batch(es) have unknown sizes and are excluded.`}</p>
    </div> : <Empty icon="drive" title="No cleanup batches yet" detail="Storage totals will appear after you move, restore or delete files through quarantine."/>}
  </section>;
}
