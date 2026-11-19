import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { Badge, CardHeading, Empty, Intro, Metric, PolicyBadge, nodeText } from "../components/Workspace";
import { usePolicy, type Policy } from "../settings";
import { PolicyChoice } from "../components/PolicyChoice";
import type { PolicyAction, Session, SimulationView } from "../types";

const PAGE_SIZE = 40;
export function SimulatorView({ visible, initialSessionId }: { visible: boolean; initialSessionId: number | null }) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionId, setSessionId] = useState(initialSessionId);
  const [override, setOverride] = useState<Policy | null>(null);
  const defaultPolicy = usePolicy();
  const template = override ?? defaultPolicy;
  const [usedTemplate, setUsedTemplate] = useState<Policy | null>(null);
  const [result, setResult] = useState<SimulationView | null>(null);
  const [kind, setKind] = useState<PolicyAction | "">("");
  const [page, setPage] = useState(0);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!visible) return;
    let live = true;
    api.listSessions().then((s) => { if (live) setSessions(s); }).catch((e: unknown) => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, [visible]);

  const load = async (offset: number) => {
    if (sessionId === null || busy) return;
    setBusy(true); setError(""); setResult(null);
    try { setResult(await api.simulate(sessionId, template, offset)); setUsedTemplate(template); setPage(offset / PAGE_SIZE); }
    catch (err) { setError(String(err)); }
    finally { setBusy(false); }
  };
  const current = result && usedTemplate === template ? result : null;
  const decisions = current?.decisions.filter((d) => !kind || d.action === kind) ?? [];

  return <section className="workspace-page" aria-labelledby="sim-title">
    <Intro id="sim-title" kicker="FILE CLEANUP" title="Cleanup policy simulator" description="See which recorded items the selected policy would keep, quarantine, or leave for review."
      actions={<Badge tone="safe" dot>Dry run only</Badge>}/>
    <section className="card tool-card" aria-label="Policy options">
      <CardHeading title="Configure dry run" icon="sliders" subtitle="Nothing changes on your computer"/>
      <form className="tool-form" onSubmit={(e) => { e.preventDefault(); setKind(""); void load(0); }}>
        <div className="field"><label htmlFor="sim-session">Installation</label><select id="sim-session" value={sessionId ?? ""} disabled={busy} onChange={(e) => { setSessionId(e.target.value ? Number(e.target.value) : null); setResult(null); }}>
          <option value="">Select a recorded installation</option>
          {sessionId !== null && !sessions.some((s) => s.session_id === sessionId) && <option value={sessionId}>Session #{sessionId}</option>}
          {sessions.map((s) => <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>)}
        </select></div>
        <PolicyChoice value={override} disabled={busy} onChange={(p) => { setOverride(p); setResult(null); }}/>
        <button type="submit" className="primary" disabled={sessionId === null || busy}><Icon name="pulse"/>{busy ? "Evaluating…" : "Run dry run"}</button>
      </form>
    </section>
    {error && <p className="notice review-notice" role="alert">{error}</p>}
    {current ? <>
      <div className="metrics three-metrics" aria-label="Policy decisions">
        <Metric icon="archive" label="Quarantine candidates" value={current.quarantine} detail="File candidates, not a deletion recommendation" tone="safe"/>
        <Metric icon="shield" label="Protected items" value={current.keep} detail="Policy says to keep these items" tone="critical"/>
        <Metric icon="alert" label="Review only" value={current.flag_for_review} detail="Uncertain or advisory findings" tone="review"/>
      </div>
      <section className="card card-overflow" aria-label="Proposed policy actions">
        <CardHeading title="Policy decisions" subtitle={current.summary} action={<label className="field"><span className="sr-only">Filter this page by action</span><select value={kind} onChange={(e) => { setKind(e.target.value as typeof kind); }}><option value="">All decisions</option><option value="quarantine">Quarantine candidates</option><option value="keep">Protected</option><option value="flag_for_review">Review only</option></select></label>}/>
        {decisions.length ? <div className="dashboard-scroll"><table className="dashboard-table policy-table"><thead><tr><th scope="col">Recorded item</th><th scope="col">Decision</th><th scope="col">Reason</th></tr></thead><tbody>
          {decisions.map((d) => <tr key={d.path}><td><code>{d.path}</code><small>{nodeText[d.node_type]}</small></td><td><PolicyBadge action={d.action} file={d.node_type === "file"}/></td><td>{d.reason}</td></tr>)}
        </tbody></table></div> : <Empty icon="sliders" title="No matching decisions"/>}
        {(page > 0 || current.next_offset !== null) && <div className="pager"><button type="button" disabled={busy || page === 0} onClick={() => void load((page - 1) * PAGE_SIZE)}>Previous decisions</button><span>Page {page + 1} of {Math.ceil(current.total / PAGE_SIZE)}</span><button type="button" disabled={busy || current.next_offset === null} onClick={() => void load(current.next_offset ?? 0)}>Next decisions</button></div>}
      </section>
      <div className="notice"><Icon name="info"/><span>A green badge means the policy nominated a file for reversible quarantine, not that deletion is safe. The engine re-checks the selection before moving files.</span></div>
    </> : <div className="card"><Empty icon="sliders" title={busy ? "Evaluating recorded evidence…" : "No preview yet"} detail="Select an installation and run a dry run to see its proposed actions."/></div>}
  </section>;
}
