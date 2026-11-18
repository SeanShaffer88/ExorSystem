import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { isDemo } from "../bridge";
import { ChangeBadge, EvidenceDiff, changeStyle, domainIcon, domainText } from "../components/ChangeEvidence";
import { Icon } from "../components/Icon";
import { comparisonLabel, hasSkipped, WatchCoverageCards, WatchWarnings } from "../components/WatchCoverage";
import { Badge, CardHeading, Empty, Intro, evidenceText, leafName } from "../components/Workspace";
import type { ChangeDomain, ChangeKind, HistoryDates, HistoryPage, HistoryRow, Session, WatchSummary } from "../types";

type Result = { key: string; page?: HistoryPage; error?: string };
function useHistoryPage(visible: boolean, path: string | null, sessionId: number | null, domain: ChangeDomain | null, limit: number, changeKind: ChangeKind | null, { fromDate, beforeDate }: HistoryDates) {
  const [cursors, setCursors] = useState<(number | null)[]>([null]);
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<Result | null>(null);
  const beforeId = cursors.at(-1) ?? null;
  const key = JSON.stringify([sessionId, path, domain, changeKind, beforeId, attempt, fromDate, beforeDate]);
  const current = result?.key === key ? result : null;
  useEffect(() => {
    if (!visible) return;
    let live = true;
    const wait = window.setTimeout(() => {
      api.history(path, limit, sessionId, beforeId, domain, changeKind, { fromDate, beforeDate }).then((page) => { if (live) setResult({ key, page }); })
        .catch((e: unknown) => { if (live) setResult({ key, error: String(e) }); });
    }, path ? 180 : 0);
    return () => { live = false; window.clearTimeout(wait); };
  }, [key, limit, path, sessionId, beforeId, domain, changeKind, fromDate, beforeDate, visible]);
  const older = current?.page?.next_before_id ?? null;
  return {
    page: current?.page, number: cursors.length, error: current?.error, loading: !current,
    retry: () => setAttempt((n) => n + 1),
    newer: cursors.length > 1 ? () => setCursors((ids) => ids.slice(0, -1)) : undefined,
    older: older === null ? undefined : () => setCursors((ids) => [...ids, older]),
  };
}
type Page = ReturnType<typeof useHistoryPage>;
const changeKinds: readonly ChangeKind[] = ["created", "modified", "deleted", "renamed", "quarantined", "restored"];

function SaveChanges({ path, sessionId, changeKind, dates }: { path: string | null; sessionId: number | null; changeKind: ChangeKind | null; dates: HistoryDates }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const saving = useRef(false);
  const [cursor, setCursor] = useState<number | null>(null);
  const save = async (before: number | null) => {
    if (saving.current) return;
    saving.current = true; setPending(true); setError(""); setMessage("");
    try {
      const result = await api.saveChanges(path, sessionId, changeKind, dates, before);
      setCursor(result.next_before_id);
      setMessage(isDemo() ? `Demo: ${result.records} observation(s) would be exported. No file was saved.`
        : `Saved ${result.records} observation(s) to ${result.path}.${result.truncated ? " Older matches can be saved in the next part." : ""}`);
    } catch (e) { setError(`Could not save matching changes: ${String(e)}`); }
    finally { saving.current = false; setPending(false); }
  };
  return <details className="card tool-card change-export">
    <summary>Export matching changes</summary>
    <p className="muted">Save matching observations as JSON in Documents, up to 1,000 per part. Each part includes all evidence domains under these filters, not just the visible page. The file contains recorded paths and fingerprints; review it before sharing.</p>
    <button type="button" className="quiet-button" disabled={pending} onClick={() => void save(null)}>{pending ? "Saving…" : "Save changes JSON"}</button>
    {cursor !== null && <button type="button" className="quiet-button" disabled={pending} onClick={() => void save(cursor)}>Save next older JSON part</button>}
    {message && <p role="status">{message}</p>}
    {error && <p role="alert" className="error">{error}</p>}
  </details>;
}

export function HistoryView({ visible, initialSessionId, initialRow, onTrace, onWatch }: {
  visible: boolean;
  initialSessionId: number | null; initialRow?: HistoryRow; onTrace: (path: string) => void; onWatch: () => void;
}) {
  const [filter, setFilter] = useState("");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionError, setSessionError] = useState("");
  const [sessionId, setSessionId] = useState(initialSessionId);
  const [changeKind, setChangeKind] = useState<ChangeKind | null>(null);
  const [fromDate, setFromDate] = useState("");
  const [beforeDate, setBeforeDate] = useState("");
  const dates = { fromDate: fromDate || null, beforeDate: beforeDate || null };
  const [openingRow, setOpeningRow] = useState(initialRow);
  const [timeline, setTimeline] = useState(initialRow?.domain === "system");
  const [refresh, setRefresh] = useState(0);
  const path = filter.trim() || null;
  const key = JSON.stringify([path, sessionId, changeKind, refresh, fromDate, beforeDate]);
  const session = sessions.find((s) => s.session_id === sessionId);
  useEffect(() => {
    if (!visible) return;
    setSessionError("");
    let live = true;
    api.listSessions().then((list) => { if (live) setSessions(list); }).catch((e: unknown) => { if (live) setSessionError(String(e)); });
    return () => { live = false; };
  }, [refresh, visible]);

  return <section className="workspace-page" aria-labelledby="history-title">
    <Intro id="history-title" kicker="SYSTEM EVIDENCE" title="Change inspector" description="Compare registry and file system observations side by side. Snapshots show what changed, not which process did it."
      actions={<><button type="button" className="quiet-button" onClick={() => { setOpeningRow(undefined); setRefresh((n) => n + 1); }}><Icon name="refresh"/> Refresh</button><button type="button" className="primary" onClick={onWatch}><Icon name="plus"/> Watch an install</button></>}/>
    <div className="card inspector-toolbar">
      <div className="field"><label htmlFor="inspect-session">Installation</label><select id="inspect-session" value={sessionId ?? ""} onChange={(e) => { setSessionId(e.target.value ? Number(e.target.value) : null); setOpeningRow(undefined); }}>
        <option value="">All installations and unassigned changes</option>
        {sessionId !== null && !sessions.some((s) => s.session_id === sessionId) && <option value={sessionId}>Session #{sessionId}</option>}
        {sessions.map((s) => <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id}){s.status === "failed" ? " · Not completed" : s.status === "monitoring" ? " · In progress" : ""}</option>)}
      </select></div>
      <label className="field inspector-search">Filter by path<span className="search-box"><Icon name="search"/><input type="search" placeholder="Search the recorded path" value={filter} onChange={(e) => { setFilter(e.target.value); setOpeningRow(undefined); }}/></span></label>
      <div className="field"><label htmlFor="inspect-kind">Change type</label><select id="inspect-kind" value={changeKind ?? ""} onChange={(e) => {
        setChangeKind(changeKinds.find((kind) => kind === e.target.value) ?? null); setOpeningRow(undefined);
      }}><option value="">All changes</option>{changeKinds.map((kind) => <option key={kind} value={kind}>{changeStyle(kind).label}</option>)}</select></div>
      <label className="field">From date (UTC)<input type="date" value={fromDate} onChange={(e) => { setFromDate(e.target.value); setOpeningRow(undefined); }}/></label>
      <label className="field">Before date (UTC, exclusive)<input type="date" value={beforeDate} onChange={(e) => { setBeforeDate(e.target.value); setOpeningRow(undefined); }}/></label>
      <div className="segments" role="group" aria-label="Inspector layout">
        <button type="button" aria-pressed={!timeline} onClick={() => setTimeline(false)}><Icon name="changes" size={16}/> Split view</button>
        <button type="button" aria-pressed={timeline} onClick={() => setTimeline(true)}><Icon name="list" size={16}/> Timeline</button>
      </div>
    </div>
    {sessionError && <p role="alert" className="error">Could not load installation names: {sessionError}</p>}
    {session?.status === "failed" && !session.ended_at && <p className="notice review-notice">This session did not finish. No ending time was recorded.</p>}
    <SaveChanges key={`export:${key}`} dates={dates} path={path} sessionId={sessionId} changeKind={changeKind}/>
    {sessionId !== null && <SavedWatchCoverage key={`${sessionId}:${refresh}`} visible={visible} sessionId={sessionId}/>}
    {timeline ? <Timeline visible={visible} key={key} dates={dates} sessionId={sessionId} path={path} changeKind={changeKind} initialRow={openingRow} onTrace={onTrace}/> : <SplitInspector visible={visible} key={key} dates={dates} sessionId={sessionId} path={path} changeKind={changeKind} initialRow={openingRow} onTrace={onTrace}/>}
  </section>;
}

type SavedCoverageResult = { summary: WatchSummary | null } | { error: string };

function SavedWatchCoverage({ visible, sessionId }: { visible: boolean; sessionId: number }) {
  const [result, setResult] = useState<SavedCoverageResult | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    api.watchComparison(sessionId).then((summary) => { if (live) setResult({ summary }); })
      .catch((e: unknown) => { if (live) setResult({ error: String(e) }); });
    return () => { live = false; };
  }, [attempt, sessionId, visible]);
  const summary = result && "summary" in result ? result.summary : null;
  return <section className="card tool-card" aria-label="Saved watch coverage">
    <CardHeading title="Watch coverage" icon="registry" tone="violet" subtitle={`Session #${sessionId} · Saved snapshot comparison`}
      action={summary && <Badge tone={hasSkipped(summary.coverage) ? "review" : "safe"} dot>{comparisonLabel(summary.coverage)}</Badge>}/>
    {!result ? <p role="status" className="muted">Loading watch coverage…</p> : "error" in result
      ? <div className="watch-result"><p className="error" role="alert">Could not load watch coverage: {result.error}</p>
        <div><button type="button" onClick={() => { setResult(null); setAttempt((n) => n + 1); }}>Retry coverage</button></div></div>
      : summary ? <div className="watch-result">
        <WatchCoverageCards coverage={summary.coverage} summary={summary}/>
        <WatchWarnings warnings={summary.warnings}/>
        <small className="muted">Counts are from the saved comparison, not the path filter or later cleanup.</small>
      </div> : <p className="muted">No saved comparison for this session. Coverage cannot be inferred from the change list.</p>}
  </section>;
}

type EvidenceProps = { visible: boolean; sessionId: number | null; path: string | null; changeKind: ChangeKind | null; dates: HistoryDates; initialRow?: HistoryRow; onTrace: (path: string) => void };

function SplitInspector({ visible, sessionId, path, changeKind, dates, initialRow, onTrace }: EvidenceProps) {
  const registry = useHistoryPage(visible, path, sessionId, "registry", 6, changeKind, dates);
  const files = useHistoryPage(visible, path, sessionId, "file", 6, changeKind, dates);
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

function Timeline({ visible, sessionId, path, changeKind, dates, initialRow, onTrace }: EvidenceProps) {
  const data = useHistoryPage(visible, path, sessionId, null, 100, changeKind, dates);
  const [selected, setSelected] = useState<HistoryRow | null>(initialRow ?? null);
  return <>
    <section className="card card-overflow" aria-label="History timeline">
      <CardHeading title="History timeline" subtitle="All evidence sources · newest first"/>
      {data.error ? <PageError data={data}/> : !data.page?.rows.length ? <Empty icon="clock" title={data.loading ? "Loading changes…" : (path || changeKind || sessionId !== null) ? "No recorded changes match these filters." : "Nothing recorded yet."}/>
        : <div className="dashboard-scroll timeline-scroll"><table className="dashboard-table timeline-table">
          <thead><tr><th scope="col">When (UTC)</th><th scope="col">Change</th><th scope="col">Path</th><th scope="col">Source</th><th scope="col">Session</th></tr></thead>
          <tbody>{data.page.rows.map((r) => <tr key={r.id} className={r.id === selected?.id ? "selected-row" : ""}>
            <td><time>{r.when ? `${r.when.replace("T", " ").replace(/Z$/, "")} UTC` : "Unknown"}</time></td><td><ChangeBadge what={r.what}/></td>
            <td><button type="button" className="row-button timeline-path" onClick={() => setSelected(r)} aria-pressed={r.id === selected?.id}>{r.path}</button></td><td>{evidenceText[r.source] ?? r.source}</td><td>{r.session_id === null ? "Unassigned" : `#${r.session_id}`}</td>
          </tr>)}</tbody>
        </table></div>}
      <Pager data={data} onPage={() => setSelected(null)}/>
    </section>
    {selected && <EvidenceDiff row={selected} onTrace={onTrace}/>}
  </>;
}
