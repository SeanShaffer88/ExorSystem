import { useEffect, useState } from "react";
import { api } from "../api";
import { ChangeBadge, EvidenceDiff, domainIcon, domainText } from "../components/ChangeEvidence";
import { Icon } from "../components/Icon";
import { Badge, CardHeading, Empty, Intro, evidenceText, leafName, timeText } from "../components/Workspace";
import type { ChangeDomain, HistoryPage, HistoryRow, Session } from "../types";

type Result = { key: string; page?: HistoryPage; error?: string };
function useHistoryPage(path: string | null, sessionId: number | null, domain: ChangeDomain | null, limit: number) {
  const [cursors, setCursors] = useState<(number | null)[]>([null]);
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<Result | null>(null);
  const beforeId = cursors.at(-1) ?? null;
  const key = JSON.stringify([sessionId, path, domain, beforeId, attempt]);
  const current = result?.key === key ? result : null;
  useEffect(() => {
    let live = true;
    const wait = window.setTimeout(() => {
      api.history(path, limit, sessionId, beforeId, domain).then((page) => { if (live) setResult({ key, page }); })
        .catch((e: unknown) => { if (live) setResult({ key, error: String(e) }); });
    }, path ? 180 : 0);
    return () => { live = false; window.clearTimeout(wait); };
  }, [key, limit, path, sessionId, beforeId, domain]);
  const older = current?.page?.next_before_id ?? null;
  return {
    page: current?.page, number: cursors.length, error: current?.error, loading: !current,
    retry: () => setAttempt((n) => n + 1),
    newer: cursors.length > 1 ? () => setCursors((ids) => ids.slice(0, -1)) : undefined,
    older: older === null ? undefined : () => setCursors((ids) => [...ids, older]),
  };
}
type Page = ReturnType<typeof useHistoryPage>;

export function HistoryView({ initialSessionId, initialRow, onTrace, onWatch }: {
  initialSessionId: number | null; initialRow?: HistoryRow; onTrace: (path: string) => void; onWatch: () => void;
}) {
  const [filter, setFilter] = useState("");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionError, setSessionError] = useState("");
  const [sessionId, setSessionId] = useState(initialSessionId);
  const [openingRow, setOpeningRow] = useState(initialRow);
  const [timeline, setTimeline] = useState(initialRow?.domain === "system");
  const [refresh, setRefresh] = useState(0);
  const path = filter.trim() || null;
  const key = JSON.stringify([path, sessionId, refresh]);
  useEffect(() => {
    let live = true;
    api.listSessions().then((list) => { if (live) setSessions(list); }).catch((e: unknown) => { if (live) setSessionError(String(e)); });
    return () => { live = false; };
  }, [refresh]);

  return <section className="workspace-page" aria-labelledby="history-title">
    <Intro id="history-title" kicker="SYSTEM EVIDENCE" title="Change inspector" description="Compare registry and file system observations side by side. Snapshots show what changed, not which process did it."
      actions={<><button type="button" className="quiet-button" onClick={() => { setOpeningRow(undefined); setRefresh((n) => n + 1); }}><Icon name="refresh"/> Refresh</button><button type="button" className="primary" onClick={onWatch}><Icon name="plus"/> Watch an install</button></>}/>
    <div className="card inspector-toolbar">
      <div className="field"><label htmlFor="inspect-session">Installation</label><select id="inspect-session" value={sessionId ?? ""} onChange={(e) => { setSessionId(e.target.value ? Number(e.target.value) : null); setOpeningRow(undefined); }}>
        <option value="">All installations and unassigned changes</option>
        {sessionId !== null && !sessions.some((s) => s.session_id === sessionId) && <option value={sessionId}>Session #{sessionId}</option>}
        {sessions.map((s) => <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>)}
      </select></div>
      <label className="field inspector-search">Filter by path<span className="search-box"><Icon name="search"/><input type="search" placeholder="Search the recorded path" value={filter} onChange={(e) => { setFilter(e.target.value); setOpeningRow(undefined); }}/></span></label>
      <div className="segments" role="group" aria-label="Inspector layout">
        <button type="button" aria-pressed={!timeline} onClick={() => setTimeline(false)}><Icon name="changes" size={16}/> Split view</button>
        <button type="button" aria-pressed={timeline} onClick={() => setTimeline(true)}><Icon name="list" size={16}/> Timeline</button>
      </div>
    </div>
    {sessionError && <p role="alert" className="error">Could not load installation names: {sessionError}</p>}
    {timeline ? <Timeline key={key} sessionId={sessionId} path={path} initialRow={openingRow} onTrace={onTrace}/> : <SplitInspector key={key} sessionId={sessionId} path={path} initialRow={openingRow} onTrace={onTrace}/>}
  </section>;
}

type EvidenceProps = { sessionId: number | null; path: string | null; initialRow?: HistoryRow; onTrace: (path: string) => void };

function SplitInspector({ sessionId, path, initialRow, onTrace }: EvidenceProps) {
  const registry = useHistoryPage(path, sessionId, "registry", 6);
  const files = useHistoryPage(path, sessionId, "file", 6);
  const [selected, setSelected] = useState<HistoryRow | null>(initialRow ?? null);
  const row = selected ?? files.page?.rows[0] ?? registry.page?.rows[0] ?? null;
  return <>
    <div className="inspector-panes">
      <InspectorPane domain="registry" data={registry} selected={row?.id} onSelect={setSelected} onPage={() => setSelected(null)}/>
      <InspectorPane domain="file" data={files} selected={row?.id} onSelect={setSelected} onPage={() => setSelected(null)}/>
    </div>
    {row && <EvidenceDiff row={row} onTrace={onTrace}/>}
    <div className="notice"><Icon name="info"/><span>Services, scheduled tasks, and unassigned observations are also available in Timeline. An added or removed badge describes a change, not clearance to delete anything.</span></div>
  </>;
}

function InspectorPane({ domain, data, selected, onSelect, onPage }: { domain: "file" | "registry"; data: Page; selected?: number; onSelect: (r: HistoryRow) => void; onPage: () => void }) {
  return <section className="card card-overflow inspector-pane" aria-label={domainText[domain]}>
    <CardHeading title={domainText[domain]} icon={domainIcon[domain]} tone={domain === "registry" ? "violet" : "blue"} subtitle={domain === "registry" ? "Key observations · read-only" : "Files and recorded fingerprints"} action={data.page && <Badge>{data.page.rows.length} on page</Badge>}/>
    {data.error ? <PageError data={data}/> : !data.page?.rows.length ? <Empty icon={domainIcon[domain]} title={data.loading ? "Loading changes…" : "No matching changes"} detail={data.loading ? undefined : "Change the filters or record a new comparison."}/>
      : <ul className="diff-list">{data.page.rows.map((r) => <li key={r.id}>
        <button type="button" className="row-button evidence-row" aria-pressed={selected === r.id} onClick={() => onSelect(r)} title={r.path}>
          <ChangeBadge what={r.what} signed/><span><strong>{leafName(r.path)}</strong><code>{r.path}</code></span><Icon name="chevron" size={15}/>
        </button>
      </li>)}</ul>}
    {(data.number > 1 || data.older) && <Pager data={data} onPage={onPage}/>}
  </section>;
}

function PageError({ data }: { data: Page }) {
  return <div className="tool-card"><p role="alert" className="error">Could not load changes: {data.error}</p><button type="button" onClick={data.retry}>Retry</button></div>;
}

function Pager({ data, onPage }: { data: Page; onPage: () => void }) {
  return <div className="pager">
    <button type="button" disabled={!data.newer} onClick={() => { onPage(); data.newer?.(); }}>Newer changes</button>
    <span>Page {data.number}</span>
    <button type="button" disabled={!data.older} onClick={() => { onPage(); data.older?.(); }}>Older changes</button>
  </div>;
}

function Timeline({ sessionId, path, initialRow, onTrace }: EvidenceProps) {
  const data = useHistoryPage(path, sessionId, null, 100);
  const [selected, setSelected] = useState<HistoryRow | null>(initialRow ?? null);
  return <>
    <section className="card card-overflow" aria-label="History timeline">
      <CardHeading title="History timeline" subtitle="All evidence sources · newest first"/>
      {data.error ? <PageError data={data}/> : !data.page?.rows.length ? <Empty icon="clock" title={data.loading ? "Loading changes…" : (path || sessionId !== null) ? "No recorded changes match these filters." : "Nothing recorded yet."}/>
        : <div className="dashboard-scroll timeline-scroll"><table className="dashboard-table timeline-table">
          <thead><tr><th scope="col">When (UTC)</th><th scope="col">Change</th><th scope="col">Path</th><th scope="col">Source</th><th scope="col">Session</th></tr></thead>
          <tbody>{data.page.rows.map((r) => <tr key={r.id} className={r.id === selected?.id ? "selected-row" : ""}>
            <td title={r.when ?? undefined}>{timeText(r.when)}</td><td><ChangeBadge what={r.what}/></td>
            <td><button type="button" className="row-button timeline-path" onClick={() => setSelected(r)} aria-pressed={r.id === selected?.id}>{r.path}</button></td><td>{evidenceText[r.source] ?? r.source}</td><td>{r.session_id === null ? "Unassigned" : `#${r.session_id}`}</td>
          </tr>)}</tbody>
        </table></div>}
      <Pager data={data} onPage={() => setSelected(null)}/>
    </section>
    {selected && <EvidenceDiff row={selected} onTrace={onTrace}/>}
  </>;
}
