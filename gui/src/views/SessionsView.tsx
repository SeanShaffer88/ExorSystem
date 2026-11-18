import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { AppMark, Badge, CardHeading, Empty, Intro, TileIcon, nodeText, programSize, scopeText, sourceText } from "../components/Workspace";
import type { InstalledProgram, ProgramInventory, Session, SessionItems } from "../types";
import { ProgramActions } from "./ProgramActions";

interface Entry {
  key: string; name: string; program: InstalledProgram | null; sessionIds: number[]; inferred?: boolean; removed: boolean;
}
const PAGE_SIZE = 16;
// The session_items command returns this many rows per page.
const ITEM_PAGE_SIZE = 12;

export function SessionsView({ visible, initialProgramId, onTrace, onHistory, onSimulate, onCleanup, onTerminal }: {
  visible: boolean;
  initialProgramId: string | null; onTrace: (path: string) => void; onHistory: (id: number) => void;
  onSimulate: (id: number) => void; onCleanup: () => void; onTerminal: (id: string) => void;
}) {
  const [inventory, setInventory] = useState<ProgramInventory | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(true);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState("name");
  const [source, setSource] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [selected, setSelected] = useState(initialProgramId ? `p:${initialProgramId}` : null);
  const [page, setPage] = useState<number | null>(null);
  const details = useRef<HTMLElement>(null);
  const back = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLElement>(null);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const returnToList = useRef(false);

  useEffect(() => {
    if (!visible) return;
    let live = true;
    setPending(true); setError("");
    Promise.all([api.listPrograms(showSystem), api.listSessions()]).then(([result, list]) => {
      if (live) { setInventory(result); setSessions(list); }
    }).catch((e: unknown) => { if (live) setError(String(e)); }).finally(() => { if (live) setPending(false); });
    return () => { live = false; };
  }, [revision, showSystem, visible]);

  const entries = useMemo<Entry[]>(() => {
    if (!inventory) return [];
    const rows = inventory.programs;
    const matched = new Set(rows.flatMap((r) => r.session_ids));
    const fromPrograms = rows.map((r) => ({ key: `p:${r.program.id}`, program: r.program, name: r.program.name, sessionIds: [...r.session_ids, ...r.inferred_session_ids], inferred: r.inferred_session_ids.length > 0, removed: r.removed }));
    const recordedOnly = sessions.filter((s) => !matched.has(s.session_id)).map((s) => ({ key: `s:${s.session_id}`, program: null, name: s.app_name, sessionIds: [s.session_id], removed: false }));
    return [...fromPrograms, ...recordedOnly].sort((a, b) => a.name.localeCompare(b.name));
  }, [inventory, sessions]);
  const shown = entries.filter((e) => {
    const p = e.program;
    const text = `${e.name} ${p?.publisher ?? ""} ${p?.version ?? ""}`.toLowerCase();
    return (!source || (p?.source ?? "recorded") === source) && text.includes(query.trim().toLowerCase());
  });
  if (sort !== "name") shown.sort((a, b) => {
    const first = a.program?.size_kb, second = b.program?.size_kb;
    if (first == null || second == null) return Number(first == null) - Number(second == null);
    return sort === "largest" ? second - first : first - second;
  });
  // Keep details stable when the user moves between result pages; changes to search clear hidden selections.
  const current = shown.find((e) => e.key === selected) ?? null;
  const base = Math.max(0, Math.floor(shown.findIndex((r) => r.key === selected) / PAGE_SIZE));
  const pages = Math.max(1, Math.ceil(shown.length / PAGE_SIZE));
  const number = Math.min(page ?? base, pages - 1);

  const currentKey = current?.key;
  useLayoutEffect(() => {
    if (!visible) return;
    if (returnToList.current) {
      returnToList.current = false;
      (trigger.current?.isConnected ? trigger.current : list.current?.querySelector("input"))?.focus();
      trigger.current = null;
    }
    if (!currentKey || !details.current) return;
    if (back.current?.getClientRects().length) {
      // A narrow dock shows a detail page instead of placing it under all sixteen rows.
      // Keep focus in the newly visible content, not on the now-hidden row.
      if (trigger.current) back.current.focus({ preventScroll: true });
      details.current.scrollIntoView({ block: "start", inline: "nearest" });
    } else {
      details.current.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  }, [currentKey, visible]);

  return <section className="workspace-page" aria-labelledby="apps-title">
    <Intro id="apps-title" kicker="INVENTORY" title="Installed applications" description="Browse classic programs, current-user packages, portable entries, and recorded installs." actions={<button type="button" className="quiet-button" disabled={pending} onClick={() => setRevision((n) => n + 1)}><Icon name="refresh"/> Refresh inventory</button>}/>
    {error && <p role="alert" className="notice review-notice">Could not load programs: {error}</p>}
    {inventory?.warnings.map((w) => <div className="notice review-notice" key={w}><Icon name="alert"/><span>{w}</span></div>)}
    <div className="inventory-layout" data-selection={!!current}>
      <section className="card card-overflow inventory-list" ref={list} aria-label="Application inventory">
        <CardHeading title="Application inventory" action={<Badge>{shown.length} result{shown.length === 1 ? "" : "s"}</Badge>}/>
        <div className="inventory-filters">
          <label className="search-box"><Icon name="search"/><span className="sr-only">Search programs</span><input type="search" placeholder="Search name, publisher or version" value={query} onChange={(e) => { setQuery(e.target.value); setPage(0); }}/></label>
          <label><span className="sr-only">Application source</span><select value={source} onChange={(e) => { setSource(e.target.value); setPage(0); }}><option value="">All sources</option><option value="registry">Classic</option><option value="msix">MSIX</option><option value="portable">Portable</option><option value="recorded">Recorded only</option></select></label>
          <label><span className="sr-only">Sort applications</span><select value={sort} onChange={(e) => { setSort(e.target.value); setPage(null); }}>
            <option value="name">Name A–Z</option><option value="largest">Largest first</option><option value="smallest">Smallest first</option>
          </select></label>
          <label className="check system-filter"><input type="checkbox" checked={showSystem} onChange={(e) => { setShowSystem(e.target.checked); setPage(0); }}/> Show system components</label>
        </div>
        {shown.length > 0 ? <div className="dashboard-scroll"><table className="dashboard-table inventory-table">
          <thead><tr><th scope="col">Application</th><th scope="col">Source</th><th scope="col" className="numeric">Size</th></tr></thead>
          <tbody>{shown.slice(number * PAGE_SIZE, (number + 1) * PAGE_SIZE).map((e) => <tr key={e.key} className={`interactive-row ${e.key === current?.key ? "selected-row" : ""}`} onClick={(event) => {
            trigger.current = event.currentTarget.querySelector("button");
            setSelected(e.key); setPage(number);
          }}>
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
      {current ? <ApplicationDetail key={current.key} entry={current} sessions={sessions} onTrace={onTrace} onHistory={onHistory} onSimulate={onSimulate} onCleanup={onCleanup} onTerminal={onTerminal} onUninstallComplete={() => setRevision((n) => n + 1)} details={details} back={back} onBack={() => {
          returnToList.current = true;
          setSelected(null);
        }}/>
        : <aside className="card inventory-empty-detail"><Empty icon="apps" title="Select an application" detail="See its Windows registration, recorded evidence, and supported actions."/></aside>}
    </div>
  </section>;
}

function ApplicationDetail({ entry, sessions, onTrace, onHistory, onSimulate, onCleanup, onTerminal, onUninstallComplete, details, back, onBack }: {
  entry: Entry; sessions: Session[]; onTrace: (path: string) => void; onHistory: (id: number) => void;
  onSimulate: (id: number) => void; onCleanup: () => void; onTerminal: (id: string) => void; onUninstallComplete: () => void;
  details: React.Ref<HTMLElement>; back: React.Ref<HTMLButtonElement>; onBack: () => void;
}) {
  const [selected, setSelected] = useState<number | null>(null);
  const sessionId = selected !== null && entry.sessionIds.includes(selected) ? selected : entry.sessionIds[0] ?? null;
  const p = entry.program;
  return <aside className="card app-detail" aria-label="Application details" ref={details} tabIndex={0}>
    <button className="quiet-button inventory-back" type="button" ref={back} onClick={onBack}>Back to applications</button>
    <div className="selected-app">{p ? <AppMark program={p}/> : <TileIcon icon="clock" tone="violet"/>}<div><div className="eyebrow">Selected application</div><h2>{entry.name}</h2></div></div>
    {p && <dl className="facts"><dt>Publisher</dt><dd>{p.publisher ?? "Unknown"}</dd><dt>Version</dt><dd>{p.version ?? "Unknown"}</dd><dt>Scope</dt><dd>{scopeText[p.scope]}</dd><dt>Location</dt><dd>{p.install_location ?? "Not reported"}</dd><dt>Reported size</dt><dd>{programSize(p.size_kb)}</dd></dl>}
    <ProgramActions key={sessionId ?? "no-session"} program={p} removed={entry.removed} sessionId={sessionId} onUninstallComplete={onUninstallComplete} onSimulate={onSimulate} onCleanup={onCleanup} onTerminal={onTerminal}/>
    <div className="session-evidence">
      <h3>Recorded evidence</h3>
      {entry.inferred && <p className="muted">Some sessions are suggested by name only. A suggestion does not authorize cleanup; link the session explicitly in Cleanup summary.</p>}
      {sessionId === null ? <p className="muted">No recorded installation is linked to this entry.</p> : <>
        {entry.sessionIds.length > 1 && <label className="field">Recorded session<select value={sessionId} onChange={(e) => setSelected(Number(e.target.value))}>{entry.sessionIds.map((id) => <option key={id} value={id}>{sessions.find((s) => s.session_id === id)?.app_name ?? "Session"} #{id}</option>)}</select></label>}
        <button className="text-button" type="button" onClick={() => onHistory(sessionId)}>Inspect session #{sessionId} <Icon name="arrow" size={15}/></button>
        <RecordedItems key={sessionId} sessionId={sessionId} onTrace={onTrace}/>
      </>}
    </div>
  </aside>;
}

function RecordedItems({ sessionId, onTrace }: { sessionId: number; onTrace: (path: string) => void }) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [retry, setRetry] = useState(0);
  const filter = query.trim() || null;
  const key = JSON.stringify([filter, page, retry]);
  const [result, setResult] = useState<{ key: string; filter: string | null; data?: SessionItems; error?: string } | null>(null);
  useEffect(() => {
    let live = true;
    api.sessionItems(sessionId, filter, page * ITEM_PAGE_SIZE).then((data) => { if (live) setResult({ key, filter, data }); })
      .catch((e: unknown) => { if (live) setResult((prev) => ({ key, filter, error: String(e), data: prev?.filter === filter ? prev?.data : undefined })); });
    return () => { live = false; };
  }, [sessionId, filter, page, key]);
  const current = result?.key === key ? result : null;
  const data = current?.data;
  const matches = result?.filter === filter ? result?.data?.matching : undefined;
  const pages = matches === undefined ? null : Math.max(1, Math.ceil(matches / ITEM_PAGE_SIZE));
  return <>
    <label className="field evidence-search">Filter recorded items<input type="search" placeholder="Search recorded paths" value={query} onChange={(e) => { setQuery(e.target.value); setPage(0); }}/></label>
    {current?.error ? <div><p role="alert" className="error">Could not load recorded items: {current.error}</p><button className="quiet-button" type="button" onClick={() => setRetry((n) => n + 1)}>Retry items</button></div>
      : !data ? <p role="status">Loading evidence…</p> : <>
        <p className="muted" role="status">{filter ? `${data.matching} of ${data.total} recorded items match` : `${data.total} recorded item(s)`}</p>
        {data.items.length ? <ul className="artifact-list">{data.items.map((i) => <li key={i.path}>
          <button type="button" className="row-button" onClick={() => onTrace(i.path)} title={i.path}>
            <span><code>{i.path}</code><small>{nodeText[i.node_type] ?? i.node_type} · {Math.round(i.confidence * 100)}% confidence</small></span>
            <Badge tone={i.shared ? "critical" : i.node_type === "file" ? "neutral" : "review"}>{i.shared ? "Shared · keep" : i.node_type === "file" ? "Unassessed" : "Read-only"}</Badge>
          </button>
        </li>)}</ul> : <p className="muted">{filter ? "No recorded items match this path filter." : "No recorded items on this page."}</p>}
      </>}
    {(page > 0 || (pages !== null && pages > 1)) && <div className="pager evidence-pager">
      <button type="button" aria-disabled={!page || (!data && !current?.error)} onClick={() => { if (page > 0 && (data || current?.error)) setPage(page - 1); }}>Previous items</button>
      <span aria-live="polite">Page {page + 1}{pages !== null ? ` of ${pages}` : ""}</span>
      <button type="button" aria-disabled={!data || !!current?.error || pages === null || page + 1 >= pages} onClick={() => { if (data && !current?.error && pages !== null && page + 1 < pages) setPage(page + 1); }}>Next items</button>
    </div>}
  </>;
}
