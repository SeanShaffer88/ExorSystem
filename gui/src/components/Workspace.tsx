import type { ReactNode } from "react";
import type { InstalledProgram, PolicyAction } from "../types";
import { Icon, type IconName } from "./Icon";

export type Tone = "neutral" | "safe" | "critical" | "review" | "violet" | "blue";

export function Badge({ tone = "neutral", children, dot = false }: { tone?: Tone; children: ReactNode; dot?: boolean }) {
  return <span className={`pill tone-${tone}`}>{dot && <i className="dot" aria-hidden="true"/>}{children}</span>;
}

export function TileIcon({ icon, tone = "safe" }: { icon: IconName; tone?: Tone }) {
  return <span className={`tile-icon tone-${tone}`}><Icon name={icon}/></span>;
}

export function Intro({ title, description, actions, id, kicker }: { title: string; description: string; actions?: ReactNode; id?: string; kicker?: string }) {
  return <header className="page-intro">
    <div>{kicker && <div className="eyebrow">{kicker}</div>}<h1 id={id}>{title}</h1><p>{description}</p></div>
    {actions && <div className="button-row">{actions}</div>}
  </header>;
}

export function CardHeading({ title, subtitle, action, icon, tone }: { title: string; subtitle?: string; action?: ReactNode; icon?: IconName; tone?: Tone }) {
  return <header className="card-heading">
    <div className="card-heading-title">{icon && <TileIcon icon={icon} tone={tone}/>}
      <div><h2>{title}</h2>{subtitle && <p>{subtitle}</p>}</div>
    </div>{action}
  </header>;
}

export function Empty({ icon = "folder", title, detail, action }: { icon?: IconName; title: string; detail?: string; action?: ReactNode }) {
  return <div className="empty-state"><TileIcon icon={icon} tone="neutral"/><strong>{title}</strong>{detail && <p>{detail}</p>}{action}</div>;
}

export function Metric({ label, value, detail, icon, tone }: { label: string; value: ReactNode; detail: string; icon: IconName; tone: Tone }) {
  return <article className="metric card">
    <div className="metric-head"><span>{label}</span><TileIcon icon={icon} tone={tone}/></div>
    <strong className="metric-value">{value}</strong>
    <p>{detail}</p>
  </article>;
}

export function AppMark({ program }: { program: InstalledProgram }) {
  const marks: [IconName, Tone][] = [["code", "blue"], ["box", "review"], ["spark", "violet"], ["apps", "safe"], ["terminal", "blue"]];
  const n = [...program.name].reduce((sum, ch) => sum + ch.charCodeAt(0), 0);
  const [icon, tone] = marks[n % marks.length]!;
  return <TileIcon icon={program.source === "msix" ? "overview" : program.source === "portable" ? "archive" : icon} tone={tone}/>;
}

export const sourceText = { registry: "Classic", msix: "MSIX", portable: "Portable" } as const;
export const scopeText = { machine: "All users", machine32: "All users (32-bit)", user: "Current user" } as const;
export const nodeText: Record<string, string> = { file: "File", registry_key: "Registry key", registry_value: "Registry value", service: "Service", task: "Scheduled task" };
export const evidenceText: Record<string, string> = {
  snapshot_diff: "Before/after snapshots", usn_journal: "File journal", registry_callback: "Registry callback",
  manifest: "Installer manifest", policy: "Cleanup policy", temporal: "Timing inference", heuristic: "Heuristic observation",
};

export const countText = (n: number) => n.toLocaleString();
export function formatBytes(n: number | null | undefined) {
  if (n == null || !Number.isFinite(n) || n < 0) return "Unknown";
  if (n < 1024) return `${countText(n)} B`;
  const exponent = Math.min(4, Math.floor(Math.log(n) / Math.log(1024)));
  return `${Number((n / 1024 ** exponent).toFixed(exponent >= 3 ? 2 : 1))} ${["B", "KiB", "MiB", "GiB", "TiB"][exponent]}`;
}
export const programSize = (kb: number | null) => kb === null ? "—" : formatBytes(kb * 1024);
export const leafName = (path: string) => path.split(/[\\/]/).pop() || path;
export const timeText = (when: string | null) => when ? `${when.slice(11, 16)} UTC` : "Unknown time";

export function PolicyBadge({ action, file }: { action: PolicyAction; file: boolean }) {
  return action === "keep" ? <Badge tone="critical" dot>Protected · keep</Badge>
    : action === "quarantine" && file ? <Badge tone="safe" dot>Quarantine candidate</Badge>
    : <Badge tone="review" dot>Review only</Badge>;
}
