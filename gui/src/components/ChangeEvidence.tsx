import type { HistoryRow } from "../types";
import { Icon } from "./Icon";
import { Badge, evidenceText, type Tone } from "./Workspace";

const changes: Record<string, { label: string; mark: string; tone: Tone }> = {
  created: { label: "Added", mark: "+", tone: "safe" },
  modified: { label: "Changed", mark: "~", tone: "review" },
  deleted: { label: "Removed", mark: "−", tone: "critical" },
  quarantined: { label: "Quarantined", mark: "↳", tone: "violet" },
  restored: { label: "Restored", mark: "↶", tone: "blue" },
  renamed: { label: "Renamed", mark: "→", tone: "blue" },
};
export const changeStyle = (what: string) => changes[what] ?? { label: what, mark: "·", tone: "neutral" as const };
export const domainText = { registry: "Registry", file: "File system", system: "Service / task" } as const;
export const domainIcon = { registry: "registry", file: "file", system: "settings" } as const;

export function ChangeBadge({ what, signed = false }: { what: string; signed?: boolean }) {
  const { label, mark, tone } = changeStyle(what);
  return <Badge tone={tone}>{signed && <span aria-hidden="true" className="change-mark">{mark}</span>}{label}</Badge>;
}

export function EvidenceDiff({ row, onTrace }: { row: HistoryRow; onTrace: (path: string) => void }) {
  const snapshot = row.source === "snapshot_diff";
  const detail = (side: "before" | "after") => {
    const before = side === "before";
    if (snapshot && ((before && row.what === "created") || (!before && row.what === "deleted"))) {
      return <span className="muted">{before ? "Not present in baseline" : "Not present in follow-up"}</span>;
    }
    if (row.domain === "registry") {
      return <span>{snapshot ? (!before && row.what === "modified" ? "Present · key last-write changed" : "Key present") : "Key state not captured"}</span>;
    }
    if (row.domain === "system") return <span>{snapshot ? "Entry present; attributes not stored" : "State not recorded"}</span>;
    const hash = before ? row.old_hash : row.new_hash;
    return hash ? <code>{hash}</code> : <span className="muted">No fingerprint recorded</span>;
  };
  return <section className="card diff-detail" aria-label="Before and after evidence">
    <header className="diff-title">
      <div><div className="eyebrow">Selected observation</div><h2>{row.path}</h2></div>
      <button className="quiet-button" onClick={() => onTrace(row.path)}><Icon name="graph"/> Trace relationships</button>
    </header>
    <div className="diff-meta"><ChangeBadge what={row.what} signed/><span>{domainText[row.domain]}</span><span>{evidenceText[row.source] ?? row.source}</span><span>{row.when ?? "Unknown time"}</span><span>{row.session_id === null ? "Unassigned" : `Session #${row.session_id}`}</span></div>
    <div className="diff-split">
      <div className={`diff-side ${row.what === "deleted" || row.what === "modified" ? "diff-minus" : ""}`}><h3>Before</h3>{detail("before")}</div>
      <div className={`diff-side ${row.what === "created" || row.what === "modified" ? "diff-plus" : ""}`}><h3>After</h3>{detail("after")}</div>
    </div>
    <p className="muted">{row.domain === "registry" ? "Registry evidence is read-only. Value names, data and raw last-write stamps are not displayed by snapshot comparison." : row.domain === "file" ? "Fingerprints are recorded evidence, not a file contents diff or a safety rating." : "Service and task evidence is for review only."}</p>
  </section>;
}
