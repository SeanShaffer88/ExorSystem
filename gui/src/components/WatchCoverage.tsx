import type { WatchCoverage, WatchSummary } from "../types";
import { Badge, countText } from "./Workspace";

const sources = [
  { key: "folders", title: "File system", unit: "folder", changes: "files_changed" },
  { key: "registry", title: "Registry", unit: "subtree", changes: "registry_keys_changed" },
  { key: "services", title: "Services", unit: null, changes: "services_changed" },
  { key: "tasks", title: "Scheduled tasks", unit: null, changes: "tasks_changed" },
] as const;

export const hasSkipped = (coverage: WatchCoverage) => Object.values(coverage).some((s) => s.usable < s.requested);
export const hasUsable = (coverage: WatchCoverage) => Object.values(coverage).some((s) => s.usable > 0);
export const comparisonLabel = (coverage: WatchCoverage) => !hasUsable(coverage) ? "No scopes compared"
  : hasSkipped(coverage) ? "Comparison incomplete" : "Comparison complete";

export function WatchCoverageCards({ coverage, summary }: { coverage: WatchCoverage; summary?: WatchSummary }) {
  return <section className="watch-coverage-panel" aria-label={summary ? "Comparison coverage" : "Baseline coverage"}>
    <dl className="watch-coverage">{sources.map(({ key, title, unit, changes }) => {
      const { requested, usable } = coverage[key];
      const incomplete = usable < requested;
      const status = !requested ? "Not requested" : !usable ? "Skipped" : incomplete ? "Partial" : summary ? "Compared" : "Baseline ready";
      return <div key={key}>
        <dt>{title}</dt>
        <dd><Badge tone={!requested ? "neutral" : incomplete ? "review" : "safe"} dot>{status}</Badge>
          {!!requested && unit && <small>{countText(usable)} of {countText(requested)} {unit}{requested === 1 ? "" : "s"} {summary ? "compared" : "ready"}</small>}
          {!!usable && summary && <small>{countText(summary[changes])} {summary[changes] === 1 ? "change" : "changes"} recorded</small>}
        </dd>
      </div>;
    })}</dl>
  </section>;
}

export function WatchWarnings({ warnings }: { warnings: string[] }) {
  return warnings.length ? <details className="notice review-notice watch-warnings">
    <summary>Skipped scope details ({warnings.length})</summary>
    <ul>{warnings.map((w, i) => <li key={i}>{w}</li>)}</ul>
  </details> : null;
}
