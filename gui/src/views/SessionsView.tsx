import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { AppMark, Badge, CardHeading, Empty, Intro, TileIcon, nodeText, programSize, scopeText, sourceText } from "../components/Workspace";
import type { InstalledProgram, ProgramInventory, Session, SessionItems } from "../types";
import { ProgramActions } from "./ProgramActions";

interface Entry {
  key: string; name: string; program: InstalledProgram | null; sessionIds: number[]; removed: boolean;
}
const PAGE_SIZE = 16;

export function SessionsView({ initialProgramId, onTrace, onHistory, onSimulate, onCleanup, onTerminal }: {
  initialProgramId: string | null; onTrace: (path: string) => void; onHistory: (id: number) => void;
  onSimulate: (id: number) => void; onCleanup: () => void; onTerminal: (id: string) => void;
}) {
  const [inventory, setInventory] = useState<ProgramInventory | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(true);
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [selected, setSelected] = useState(initialProgramId ? `p:${initialProgramId}` : null);
  const [page, setPage] = useState<number | null>(null);

  useEffect(() => {
    let live = true;
    setPending(true); setError("");
    Promise.all([api.listPrograms(showSystem), api.listSessions()]).then(([result, list]) => {
      if (live) { setInventory(result); setSessions(list); }
    }).catch((e: unknown) => { if (live) setError(String(e)); }).finally(() => { if (live) setPending(false); });
    return () => { live = false; };
  }, [revision, showSystem]);

  const entries = useMemo<Entry[]>(() => {
    if (!inventory) return [];
    const rows = inventory.programs;
    const matched = new Set(rows.flatMap((r) => r.session_ids));
    const fromPrograms = rows.map((r) => ({ key: `p:${r.program.id}`, program: r.program, name: r.program.name, sessionIds: r.session_ids, removed: r.removed }));
    const recordedOnly = sessions.filter((s) => !matched.has(s.session_id)).map((s) => ({ key: `s:${s.session_id}`, program: null, name: s.app_name, sessionIds: [s.session_id], removed: false }));
    return [...fromPrograms, ...recordedOnly].sort((a, b) => a.name.localeCompare(b.name));
  }, [inventory, sessions]);
  const shown = entries.filter((e) => {
    const p = e.program;
    const text = `${e.name} ${p?.publisher ?? ""} ${p?.version ?? ""}`.toLowerCase();
    return (!source || (p?.source ?? "recorded") === source) && text.includes(query.trim().toLowerCase());
  });
  // Keep details stable when the user moves between result pages; changes to search clear hidden selections.
  const current = shown.find((e) => e.key === selected) ?? null;
  const base = Math.max(0, Math.floor(shown.findIndex((r) => r.key === selected) / PAGE_SIZE));
  const pages = Math.max(1, Math.ceil(shown.length / PAGE_SIZE));
  const number = Math.min(page ?? base, pages - 1);

  return <section className="workspace-page" aria-labelledby="apps-title">
    <Intro id="apps-title" kicker="INVENTORY" title="Installed applications" description="Browse classic programs, current-user packages, portable entries, and recorded installs." actions={<button type="button" className="quiet-button" disabled={pending} onClick={() => setRevision((n) => n + 1)}><Icon name="refresh"/> Refresh inventory</button>}/>
    {error && <p role="alert" className="notice review-notice">Could not load programs: {error}</p>}
    {inventory?.warnings.map((w) => <div className="notice review-notice" key={w}><Icon name="alert"/><span>{w}</span></div>)}
    <div className="inventory-layout">
      <section className="card card-overflow" aria-label="Application inventory">
        <CardHeading title="Application inventory" action={<Badge>{shown.length} result{shown.length === 1 ? "" : "s"}</Badge>}/>
        <div className="inventory-filters">
          <label className="search-box"><Icon name="search"/><span className="sr-only">Search programs</span><input type="search" placeholder="Search name, publisher or version" value={query} onChange={(e) => { setQuery(e.target.value); setPage(0); }}/></label>
          <label><span className="sr-only">Application source</span><select value={source} onChange={(e) => { setSource(e.target.value); setPage(0); }}><option value="">All sources</option><option value="registry">Classic</option><option value="msix">MSIX</option><option value="portable">Portable</option><option value="recorded">Recorded only</option></select></label>
          <label className="check system-filter"><input type="checkbox" checked={showSystem} onChange={(e) => { setShowSystem(e.target.checked); setPage(0); }}/> Show system components</label>
        </div>
        {shown.length > 0 ? <div className="dashboard-scroll"><table className="dashboard-table inventory-table">
          <thead><tr><th scope="col">Application</th><th scope="col">Source</th><th scope="col" className="numeric">Size</th></tr></thead>
          <tbody>{shown.slice(number * PAGE_SIZE, (number + 1) * PAGE_SIZE).map((e) => <tr key={e.key} className={`interactive-row ${e.key === current?.key ? "selected-row" : ""}`} onClick={() => { setSelected(e.key); setPage(number); }}>
            <td><button type="button" className="app-cell row-button" aria-pressed={e.key === current?.key}>
              {e.program ? <AppMark program={e.program}/> : <TileIcon icon="clock" tone="violet"/>}
              <span><strong>{e.name}</strong><small>{e.removed ? "Recently removed" : e.program ? [e.program.publisher, e.program.version].filter(Boolean).join(" · ") || "Version not reported" : "Recorded installation"}</small></span>
            </button></td>
            <td><Badge tone={e.removed ? "review" : "neutral"}>{e.removed ? "Removed" : e.program ? sourceText[e.program.source] : "Recorded"}</Badge></td>
            <td className="numeric">{programSize(e.program?.size_kb ?? null)}</td>
          </tr>)}</tbody>
        </table></div> : <Empty icon="apps" title={pending ? "Loading inventory…" : "No matching applications"} detail="Try another search or include a different source."/>}
        {pages > 1 && <div className="pager"><button type="button" disabled={number === 0} onClick={() => setPage(number - 1)}>Previous</button><span>Page {number + 1} of {pages}</span><button type="button" disabled={number + 1 >= pages} onClick={() => setPage(number + 1)}>Next</button></div>}
      </section>
      {current ? <ApplicationDetail key={current.key} entry={current} sessions={sessions} onTrace={onTrace} onHistory={onHistory} onSimulate={onSimulate} onCleanup={onCleanup} onTerminal={onTerminal} onUninstallComplete={() => setRevision((n) => n + 1)}/>
        : <aside className="card"><Empty icon="apps" title="Select an application" detail="See its Windows registration, recorded evidence, and supported actions."/></aside>}
    </div>
  </section>;
}

function ApplicationDetail({ entry, sessions, onTrace, onHistory, onSimulate, onCleanup, onTerminal, onUninstallComplete }: {
  entry: Entry; sessions: Session[]; onTrace: (path: string) => void; onHistory: (id: number) => void;
  onSimulate: (id: number) => void; onCleanup: () => void; onTerminal: (id: string) => void; onUninstallComplete: () => void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const sessionId = selected !== null && entry.sessionIds.includes(selected) ? selected : entry.sessionIds[0] ?? null;
  const [result, setResult] = useState<{ id: number; items?: SessionItems; error?: string } | null>(null);
  useEffect(() => {
    if (sessionId === null) return;
    let live = true;
    api.sessionItems(sessionId).then((items) => { if (live) setResult({ id: sessionId, items }); })
      .catch((e: unknown) => { if (live) setResult({ id: sessionId, error: String(e) }); });
    return () => { live = false; };
  }, [sessionId]);
  const info = result?.id === sessionId ? result : null;
  const p = entry.program;
  return <aside className="card app-detail" aria-label="Application details">
    <div className="selected-app">{p ? <AppMark program={p}/> : <TileIcon icon="clock" tone="violet"/>}<div><div className="eyebrow">Selected application</div><h2>{entry.name}</h2></div></div>
    {p && <dl className="facts"><dt>Publisher</dt><dd>{p.publisher ?? "Unknown"}</dd><dt>Version</dt><dd>{p.version ?? "Unknown"}</dd><dt>Scope</dt><dd>{scopeText[p.scope]}</dd><dt>Location</dt><dd>{p.install_location ?? "Not reported"}</dd><dt>Reported size</dt><dd>{programSize(p.size_kb)}</dd></dl>}
    <ProgramActions key={sessionId ?? "no-session"} program={p} removed={entry.removed} sessionId={sessionId} onUninstallComplete={onUninstallComplete} onSimulate={onSimulate} onCleanup={onCleanup} onTerminal={onTerminal}/>
    <div className="session-evidence">
      <h3>Recorded evidence</h3>
      {sessionId === null ? <p className="muted">No recorded installation is linked to this entry.</p> : <>
        {entry.sessionIds.length > 1 && <label className="field">Recorded session<select value={sessionId} onChange={(e) => setSelected(Number(e.target.value))}>{entry.sessionIds.map((id) => <option key={id} value={id}>{sessions.find((s) => s.session_id === id)?.app_name ?? "Session"} #{id}</option>)}</select></label>}
        <button className="text-button" type="button" onClick={() => onHistory(sessionId)}>Inspect session #{sessionId} <Icon name="arrow" size={15}/></button>
        {info?.error ? <p role="alert" className="error">{info.error}</p> : !info?.items ? <p role="status">Loading evidence…</p> : <>
          <p className="muted">{info.items.total} recorded item(s) · {info.items.shared} possibly shared</p>
          <ul className="artifact-list">{info.items.items.slice(0, 12).map((i) => <li key={i.path}><button type="button" className="row-button" onClick={() => onTrace(i.path)} title={i.path}><span><code>{i.path}</code><small>{nodeText[i.node_type] ?? i.node_type} · {Math.round(i.confidence * 100)}% confidence</small></span><Badge tone={i.shared ? "critical" : i.node_type === "file" ? "neutral" : "review"}>{i.shared ? "Shared · keep" : i.node_type === "file" ? "Unassessed" : "Read-only"}</Badge></button></li>)}</ul>
          {info.items.total > 12 && <small className="muted">Showing the first 12 items. Open the session inspector for recorded changes.</small>}
        </>}
      </>}
    </div>
  </aside>;
}
