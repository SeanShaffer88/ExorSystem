import { useEffect, useState, useSyncExternalStore } from "react";
import { api, sessionChanges } from "../api";
import { Icon } from "../components/Icon";
import { comparisonLabel, hasSkipped } from "../components/WatchCoverage";
import { Badge, CardHeading, countText, Empty, Intro } from "../components/Workspace";
import type { Session, WatchArchivePage, WatchArchiveRow } from "../types";

type Criteria = { search: string; status: Session["status"] };
type Request = Criteria & { cursors: (number | null)[] };
type Result = { request: Request; revision: number } & ({ page: WatchArchivePage } | { error: string });
const allSessions: Criteria = { search: "", status: null };

export function WatchArchiveView({ visible, onInspect }: { visible: boolean; onInspect: (run: WatchArchiveRow) => void }) {
  const [draft, setDraft] = useState(allSessions);
  const [request, setRequest] = useState<Request>({ ...allSessions, cursors: [null] });
  const [result, setResult] = useState<Result | null>(null);
  const revision = useSyncExternalStore(sessionChanges.subscribe, sessionChanges.getSnapshot);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    api.watchArchive({ search: request.search || null, status: request.status, beforeId: request.cursors.at(-1) ?? null })
      .then((page) => { if (live) setResult({ request, revision, page }); })
      .catch((error: unknown) => { if (live) setResult({ request, revision, error: String(error) }); });
    return () => { live = false; };
  }, [visible, request, revision]);

  const current = result?.request === request && result.revision === revision ? result : null;
  const page = current && "page" in current ? current.page : null;
  const older = page?.next_before_id;
  const newer = request.cursors.length > 1;
  const filtered = !!request.search || !!request.status;
  const clear = () => { setDraft(allSessions); setRequest({ ...allSessions, cursors: [null] }); };

  return <section className="workspace-page" aria-labelledby="watch-archive-title">
    <Intro id="watch-archive-title" kicker="SAVED EVIDENCE" title="Watch archive"
      description="Find older sessions and open their recorded evidence in Change inspector."
      actions={<button type="button" className="quiet-button" onClick={() => setRequest({ ...request, cursors: [null] })}><Icon name="refresh"/> Refresh archive</button>}/>
    <form className="card tool-card archive-search" role="search" aria-label="Search Watch archive" onSubmit={(event) => {
      event.preventDefault(); setRequest({ ...draft, search: draft.search.trim(), cursors: [null] });
    }}>
      <div className="archive-fields">
        <label className="field grow">Application name or session ID
          <span className="search-box"><Icon name="search"/><input type="search" value={draft.search} maxLength={240}
            aria-describedby="archive-search-hint" placeholder="Example Editor or #123" onChange={(event) => setDraft({ ...draft, search: event.target.value })}/></span>
        </label>
        <div className="field"><label htmlFor="archive-status">Session status</label>
          <select id="archive-status" value={draft.status ?? ""} onChange={(event) => setDraft({ ...draft, status: (event.target.value || null) as Session["status"] })}>
            <option value="">All statuses</option>
            <option value="completed">Completed</option>
            <option value="failed">Not completed</option>
            <option value="monitoring">In progress</option>
          </select>
        </div>
        <div className="button-row"><button type="submit" className="primary">Search</button>
          <button type="button" className="quiet-button" onClick={clear}>Clear filters</button></div>
      </div>
      <p id="archive-search-hint" className="muted">Enter part of an application name or use # for an exact session ID. Search matches literal text, ignoring A–Z case.</p>
    </form>
    <section className="card card-overflow" aria-label="Watch archive results">
      <CardHeading title="Recorded sessions" subtitle="Newest sessions first" icon="archive" tone="violet"/>
      <p className="archive-result-status muted" role="status">{!current ? "Loading sessions…" : page ?
        `Page ${request.cursors.length} · ${page.rows.length} ${page.rows.length === 1 ? "session" : "sessions"}${filtered ? " matching filters" : ""}` : "Search unavailable"}</p>
      {current && "error" in current && <div className="tool-card">
        <p className="error" role="alert">Could not load the Watch archive: {current.error}</p>
        <button type="button" className="quiet-button" onClick={() => setRequest({ ...request })}>Retry archive page</button>
      </div>}
      <nav className="pager archive-pager" aria-label="Watch archive pages">
        <button type="button" aria-disabled={!newer || !current} onClick={() => {
          if (newer && current) setRequest({ ...request, cursors: request.cursors.slice(0, -1) });
        }}><Icon name="back" size={15}/> Newer sessions</button>
        <button type="button" aria-disabled={older == null} onClick={() => {
          if (older != null) setRequest({ ...request, cursors: [...request.cursors, older] });
        }}>Older sessions <Icon name="arrow" size={15}/></button>
      </nav>
      {page && (page.rows.length ? <ul className="diff-list watch-run-list">{page.rows.map((run) =>
        <ArchiveRun key={run.session_id} run={run} onInspect={onInspect}/>)}</ul> :
        <Empty icon="archive" title={filtered ? "No sessions match these filters" : "No sessions on this page"}
          detail={filtered ? "Try another application name or status." : "Only sessions that were recorded in this profile can appear here."}/>)}
    </section>
    <p className="muted archive-note">Older installation records may not have Watch coverage. Cancelled or interrupted sessions can be marked Not completed without a saved comparison.</p>
  </section>;
}

function ArchiveRun({ run, onInspect }: { run: WatchArchiveRow; onInspect: (run: WatchArchiveRow) => void }) {
  const { session_id, app_name, started_at, ended_at, status, comparison } = run;
  const lifecycle = status === "monitoring" ? "In progress" : status === "completed" ? "Completed" : status === "failed" ? "Not completed" : "Unknown status";
  const coverage = comparison ? comparisonLabel(comparison.coverage) : status === "failed" || status === "monitoring" ? "No saved comparison" : "Coverage unknown";
  const partial = comparison && (comparison.recorded_changes === null || hasSkipped(comparison.coverage));
  const changes = comparison?.recorded_changes;
  return <li><button className="row-button" type="button" onClick={() => onInspect(run)}>
    <span className="archive-run-name"><strong>{app_name}</strong>
      <small>#{session_id}{started_at && ` · Started ${started_at.slice(0, 16)} UTC`}</small>
      {ended_at ? <small>Ended {ended_at.slice(0, 16)} UTC</small> : status === "failed" && <small>No ending time recorded</small>}
    </span>
    <span className="archive-run-status">
      <Badge tone={status === "failed" ? "review" : "neutral"}>{lifecycle}</Badge>
      <Badge tone={comparison ? partial ? "review" : "safe" : "neutral"}>{coverage}</Badge>
      {changes != null && <small>{countText(changes)} recorded {changes === 1 ? "change" : "changes"}</small>}
    </span>
    <Icon name="chevron" size={15}/>
  </button></li>;
}
