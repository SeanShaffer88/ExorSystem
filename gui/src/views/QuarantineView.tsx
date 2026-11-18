import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../api";
import { BatchReview } from "../components/BatchReview";
import { Icon } from "../components/Icon";
import { StorageSummary } from "../components/StorageSummary";
import { Badge, CardHeading, Empty, Intro, formatBytes, type Tone } from "../components/Workspace";
import { getPolicy } from "../settings";
import type { QuarantineRow, Session } from "../types";

const states: Record<string, { label: string; tone: Tone }> = {
  quarantined: { label: "In quarantine", tone: "violet" }, restored: { label: "Restored", tone: "blue" },
  purged: { label: "Deleted permanently", tone: "neutral" }, rolled_back: { label: "Rolled back", tone: "blue" },
  in_progress: { label: "Interrupted", tone: "review" },
};
type Confirmation = { kind: "move"; sessionId: number; policy: string } | { kind: "purge"; id: string };

export function QuarantineView({ visible, onSimulate }: { visible: boolean; onSimulate: () => void }) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [template, setTemplate] = useState(getPolicy());
  const [recoveryWarnings, setRecoveryWarnings] = useState<string[]>([]);
  const [rows, setRows] = useState<QuarantineRow[] | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [confirm, setConfirm] = useState<Confirmation | null>(null);
  const [review, setReview] = useState<{ id: string; restore: boolean } | null>(null);
  const reviewTrigger = useRef<HTMLButtonElement | null>(null);
  const [busy, setBusy] = useState(false);
  const [revision, refresh] = useState(0);
  const gate = useRef(false);
  const reviewing = rows?.find((r) => r.quarantine_id === review?.id);
  const locked = busy || !!confirm || !!review?.restore;

  useLayoutEffect(() => {
    if (visible && !review && !busy && reviewTrigger.current) {
      const trigger = reviewTrigger.current;
      const target = trigger.isConnected ? trigger : document.getElementById("batch-list-title");
      target?.focus(); reviewTrigger.current = null;
    }
  }, [visible, review, busy]);

  useEffect(() => {
    if (!visible) { setConfirm(null); setReview(null); reviewTrigger.current = null; return; }
    setLoadError("");
    let live = true;
    api.listSessions().then((list) => { if (live) setSessions(list); }).catch((e: unknown) => { if (live) setLoadError(String(e)); });
    api.appInfo().then((info) => { if (live) setRecoveryWarnings(info.recovery_warnings ?? []); }).catch((e: unknown) => { if (live) setLoadError(String(e)); });
    return () => { live = false; };
  }, [visible]);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    api.listQuarantines().then((next) => { if (live) setRows(next); })
      .catch((e: unknown) => { if (live) setError(`Could not load batches: ${String(e)}`); });
    return () => { live = false; };
  }, [revision, visible]);

  const act = async (fn: () => Promise<string>) => {
    if (gate.current) return;
    gate.current = true; setBusy(true); setError(""); setMessage(""); setConfirm(null); setReview(null);
    try {
      setMessage(await fn());
      try { setRows(await api.listQuarantines()); } catch (e) { setRows(null); setError(`Could not refresh cleanup totals: ${String(e)}`); }
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); gate.current = false; }
  };

  const openBatch = (id: string, restore: boolean, trigger: HTMLButtonElement) => {
    reviewTrigger.current = trigger;
    setReview({ id, restore });
  };

  return <section className="workspace-page" aria-labelledby="cleanup-title">
    <Intro id="cleanup-title" kicker="FILE CLEANUP" title="Cleanup summary" description="Browse recorded batches and original paths. Review held files before restoring or permanently deleting them."
      actions={<button className="quiet-button" type="button" disabled={locked} onClick={() => { setError(""); refresh((n) => n + 1); }}><Icon name="refresh"/> Refresh batches</button>}/>
    {!!recoveryWarnings.length && <div role="alert" className="notice review-notice"><Icon name="alert"/><div><strong>Some batches need attention. Affected files were left in place.</strong><ul>{recoveryWarnings.map((w, i) => <li key={i}>{w}</li>)}</ul></div></div>}
    {loadError && <p role="alert" className="notice review-notice">Could not load all cleanup information: {loadError}</p>}
    {error && <p role="alert" className="notice review-notice">{error}</p>}
    {(busy || message) && <p role="status" className="notice success-notice">{busy ? "Working on the selected batch…" : message}</p>}
    <div className="overview-columns">
      {rows ? <StorageSummary rows={rows}/> : <div className="card"><Empty icon="drive" title={error ? "Cleanup totals unavailable" : "Loading storage summary…"}/></div>}
      <section className="card tool-card" aria-label="Prepare quarantine">
        <CardHeading title="Prepare quarantine" icon="archive" tone="violet" subtitle="Move policy-selected files from a recorded session"/>
        <form className="tool-form quarantine-form" onSubmit={(e) => {
          e.preventDefault();
          if (sessionId !== null && !locked) {
            reviewTrigger.current = null; setReview(null); setConfirm({ kind: "move", sessionId, policy: template });
          }
        }}>
          <div className="field grow"><label htmlFor="cleanup-session">Installation</label><select id="cleanup-session" value={sessionId ?? ""} disabled={locked} onChange={(e) => setSessionId(e.target.value ? Number(e.target.value) : null)}>
            <option value="">Select a recorded installation</option>
            {sessions.map((s) => <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>)}
          </select></div>
          <div className="field"><label htmlFor="cleanup-policy">Policy</label><select id="cleanup-policy" value={template} disabled={locked} onChange={(e) => setTemplate(e.target.value)}><option value="conservative">Conservative</option><option value="standard">Standard</option><option value="thorough">Thorough</option></select></div>
          <div className="button-row">
            <button type="button" className="quiet-button" onClick={onSimulate} disabled={locked}>Open policy simulator</button>
            <button type="submit" className="primary" disabled={sessionId === null || locked}>Move to quarantine…</button>
          </div>
        </form>
        {confirm?.kind === "move" && <div className="notice review-notice confirm-box" role="group" aria-label="Confirm move to quarantine"><div><strong>Quarantine files selected by {confirm.policy} for session #{confirm.sessionId}?</strong><p>You can restore held files. System entries are not changed.</p></div><div className="button-row">
          <button type="button" className="quiet-button" onClick={() => setConfirm(null)}>Cancel</button>
          <button type="button" className="primary" onClick={() => void act(async () => (await api.quarantineSession(confirm.sessionId, confirm.policy)).summary)}>Confirm: move files</button>
        </div></div>}
      </section>
    </div>
    <section className="card card-overflow" id="batch-list-title" tabIndex={-1} aria-label="Quarantine batches">
      <CardHeading title="Quarantine batches" subtitle="Files stay on disk until permanently deleted" action={rows && <Badge>{rows.length} batch{rows.length === 1 ? "" : "es"}</Badge>}/>
      {rows?.length ? <div className="dashboard-scroll"><table className="dashboard-table batch-table">
        <thead><tr><th scope="col">Batch</th><th scope="col">Installation</th><th scope="col" className="numeric">Files</th><th scope="col" className="numeric">Recorded size</th><th scope="col">Status</th><th scope="col">Actions</th></tr></thead>
        <tbody>{rows.map((r) => <tr key={r.quarantine_id}>
          <td><code>{r.quarantine_id}</code><button className="text-button batch-view" type="button" disabled={locked} onClick={(e) => openBatch(r.quarantine_id, false, e.currentTarget)}>View batch</button></td><td>{r.session_id === null ? "Unassigned" : sessions.find((s) => s.session_id === r.session_id)?.app_name ?? `#${r.session_id}`}</td>
          <td className="numeric">{r.file_bytes == null && !r.files ? "—" : r.files}</td><td className="numeric">{formatBytes(r.file_bytes)}</td>
          <td><Badge tone={states[r.status]?.tone} dot>{states[r.status]?.label ?? r.status}</Badge></td>
          <td>{confirm?.kind === "purge" && confirm.id === r.quarantine_id ? <div role="group" aria-label="Confirm permanent delete" className="confirm-purge">
            <strong>This cannot be undone.</strong><div className="button-row"><button type="button" className="quiet-button" onClick={() => setConfirm(null)}>Cancel</button><button type="button" className="danger-button" onClick={() => void act(async () => `Deleted ${await api.purgeQuarantine(r.quarantine_id)} item(s) permanently.`)}>Confirm: delete</button></div>
          </div> : r.status === "quarantined" ? <div className="button-row">
            <button type="button" className="quiet-button" disabled={locked} onClick={(e) => openBatch(r.quarantine_id, true, e.currentTarget)}>Restore…</button>
            <button type="button" className="text-button danger-text" disabled={locked} onClick={() => { reviewTrigger.current = null; setReview(null); setConfirm({ kind: "purge", id: r.quarantine_id }); }}>Delete permanently…</button>
          </div> : null}</td>
        </tr>)}</tbody>
      </table></div> : <Empty icon="archive" title={rows ? "Nothing has been quarantined yet" : error ? "Batches unavailable" : "Loading batches…"}/>}
    </section>
    {visible && reviewing && review && <BatchReview key={review.id} row={reviewing}
      state={states[reviewing.status] ?? { label: reviewing.status, tone: "neutral" }} reviewRestore={review.restore}
      onClose={() => setReview(null)} onReviewRestore={() => { if (!locked) setReview({ id: review.id, restore: true }); }}
      onRestore={() => { if (review.restore && !confirm && reviewing.status === "quarantined") {
        void act(async () => `Restored ${await api.restoreQuarantine(review.id)} item(s).`);
      } }}/>}
  </section>;
}
