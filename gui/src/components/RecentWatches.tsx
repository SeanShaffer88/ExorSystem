import { useEffect, useState } from "react";
import { api } from "../api";
import type { WatchRun } from "../types";
import { comparisonLabel, hasSkipped, hasUsable } from "./WatchCoverage";
import { Badge, CardHeading, Empty } from "./Workspace";

export function RecentWatches({ visible, comparedId, onInspect, onArchive }: { visible: boolean; comparedId?: number; onInspect: (sessionId: number) => void; onArchive: () => void }) {
  const [runs, setRuns] = useState<WatchRun[] | null>(null);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    setError("");
    api.recentWatchRuns().then((result) => { if (live) setRuns(result); })
      .catch((e: unknown) => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, [visible, comparedId, retry]);

  return <section className="card card-overflow watch-recent" aria-label="Recent watch comparisons">
    <CardHeading title="Recent comparisons" subtitle="Saved Watch results · newest sessions first"
      action={<button type="button" className="quiet-button" onClick={onArchive}>Browse Watch archive</button>}/>
    {error ? <div className="tool-card"><p role="alert" className="error">Could not load recent comparisons: {error}</p><button type="button" onClick={() => setRetry((n) => n + 1)}>Retry comparisons</button></div>
      : runs?.length ? <ul className="diff-list watch-run-list">{runs.map(({ app_name, ended_at, summary }) => {
        const { coverage, session_id } = summary;
        const changes = summary.files_changed + summary.registry_keys_changed + summary.services_changed + summary.tasks_changed;
        return <li key={session_id}><button className="row-button" type="button" onClick={() => onInspect(session_id)}>
          <span><strong>{app_name} · #{session_id}</strong><small>{ended_at ? `${ended_at.slice(0, 16)} UTC · ` : ""}{hasUsable(coverage) ? `${changes} recorded change${changes === 1 ? "" : "s"}` : "Coverage only"}</small></span>
          <Badge tone={hasSkipped(coverage) ? "review" : "safe"}>{comparisonLabel(coverage)}</Badge>
        </button></li>;
      })}</ul> : <Empty icon="clock" title={runs ? "No saved comparisons" : "Loading comparisons…"} detail={runs ? "Saved Watch results appear here even when nothing changed." : undefined}/>}
  </section>;
}
