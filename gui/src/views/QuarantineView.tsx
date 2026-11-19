import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../api";
import { BatchReview } from "../components/BatchReview";
import { BrowsePath } from "../components/BrowsePath";
import { CleanupPlanReview } from "../components/CleanupPlanReview";
import { Icon } from "../components/Icon";
import { PolicyChoice } from "../components/PolicyChoice";
import { StorageSummary } from "../components/StorageSummary";
import { Badge, CardHeading, Empty, Intro, formatBytes, type Tone } from "../components/Workspace";
import { usePolicy, type Policy } from "../settings";
import type { LeftoverFolder, PlanHeader, ProgramRow, QuarantinePage, Session } from "../types";

const states: Record<string, { label: string; tone: Tone }> = {
  quarantined: { label: "In quarantine", tone: "violet" }, restored: { label: "Restored", tone: "blue" },
  purged: { label: "Deleted locally", tone: "neutral" }, rolled_back: { label: "Rolled back", tone: "blue" },
  restoring: { label: "Restore interrupted", tone: "review" }, purging: { label: "Delete interrupted", tone: "review" },
  in_progress: { label: "Interrupted", tone: "review" },
};

export function QuarantineView({ visible, onSimulate }: { visible: boolean; onSimulate: () => void }) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [programs, setPrograms] = useState<ProgramRow[]>([]);
  const [programId, setProgramId] = useState("");
  const [folderChoices, setFolderChoices] = useState<LeftoverFolder[] | null>(null);
  const [folders, setFolders] = useState<string[]>([]);
  const [link, setLink] = useState(false);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [override, setOverride] = useState<Policy | null>(null);
  const defaultPolicy = usePolicy();
  const [plan, setPlan] = useState<PlanHeader | null>(null);
  const [recoveryWarnings, setRecoveryWarnings] = useState<string[]>([]);
  const [page, setPage] = useState<QuarantinePage | null>(null);
  const [offset, setOffset] = useState(0);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [confirm, setConfirm] = useState<{ kind: "purge" | "rotate"; id: string } | null>(null);
  const [archive, setArchive] = useState("");
  const [review, setReview] = useState<{ id: string; restore: boolean } | null>(null);
  const reviewTrigger = useRef<HTMLButtonElement | null>(null);
  const confirmDialog = useRef<HTMLDialogElement>(null);
  const cancelConfirm = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState<"batch" | "folders" | "plan" | null>(null);
  const [revision, refresh] = useState(0);
  const gate = useRef(false);
  const rows = page?.rows;
  const reviewing = rows?.find((r) => r.quarantine_id === review?.id);
  const registration = programs.find((p) => p.program.id === programId);
  const linked = sessionId !== null && registration?.session_ids.includes(sessionId);
  const locked = !!(busy || confirm || review?.restore || plan);

  useLayoutEffect(() => {
    if (visible && !review && !busy && !confirm && !plan && reviewTrigger.current) {
      const trigger = reviewTrigger.current;
      const target = trigger.isConnected && !trigger.disabled ? trigger : document.getElementById("batch-list-title");
      target?.focus(); reviewTrigger.current = null;
    }
  }, [visible, review, busy, confirm, plan]);
  useLayoutEffect(() => {
    if (!confirm) return;
    const dialog = confirmDialog.current;
    if (dialog && !dialog.open) dialog.showModal();
    cancelConfirm.current?.focus();
    return () => dialog?.close();
  }, [confirm]);
  useEffect(() => {
    if (!visible) { setConfirm(null); setReview(null); reviewTrigger.current = null; return; }
    setLoadError("");
    let live = true;
    api.listSessions().then((list) => { if (live) setSessions(list); }).catch((e: unknown) => { if (live) setLoadError(String(e)); });
    api.listPrograms(false).then((list) => { if (live) setPrograms(list.programs.filter((p) => p.program.source === "registry")); }).catch((e: unknown) => { if (live) setLoadError(String(e)); });
    api.appInfo().then((info) => { if (live) setRecoveryWarnings(info.recovery_warnings ?? []); }).catch((e: unknown) => { if (live) setLoadError(String(e)); });
    return () => { live = false; };
  }, [visible]);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    api.quarantinePage(offset).then((next) => { if (live) setPage(next); })
      .catch((e: unknown) => { if (live) setError(`Could not load batches: ${String(e)}`); });
    return () => { live = false; };
  }, [revision, visible, offset]);

  const work = async (kind: typeof busy, fn: () => Promise<void>) => {
    if (gate.current) return;
    gate.current = true; setBusy(kind); setError(""); setMessage("");
    try { await fn(); } catch (e) { setError(String(e)); }
    finally { setBusy(null); gate.current = false; }
  };
  const act = (fn: () => Promise<string>) => void work("batch", async () => {
    setConfirm(null); setReview(null);
    try { setMessage(await fn()); } finally { refresh((n) => n + 1); }
  });
  const openBatch = (id: string, restore: boolean, trigger: HTMLButtonElement) => {
    reviewTrigger.current = trigger;
    setReview({ id, restore });
  };
  const confirmBatch = (kind: "purge" | "rotate", id: string, trigger: HTMLButtonElement) => {
    reviewTrigger.current = trigger; setReview(null); setArchive(""); setConfirm({ kind, id });
  };
  return <section className="workspace-page" aria-labelledby="cleanup-title">
    <Intro id="cleanup-title" kicker="FILE CLEANUP" title="Cleanup summary" description="Review an exact file plan before moving anything. Browse held batches, restore files, or rotate a verified archive."
      actions={<button className="quiet-button" type="button" disabled={locked} onClick={() => { setError(""); refresh((n) => n + 1); }}><Icon name="refresh"/> Refresh batches</button>}/>
    {!!recoveryWarnings.length && <div role="alert" className="notice review-notice"><Icon name="alert"/><div><strong>Some batches need attention.</strong><ul>{recoveryWarnings.map((w, i) => <li key={i}>{w}</li>)}</ul></div></div>}
    {loadError && <p role="alert" className="notice review-notice">Could not load all cleanup information: {loadError}</p>}
    {error && <p role="alert" className="notice review-notice">{error}</p>}
    {(busy || message) && <p role="status" className="notice success-notice">{busy === "plan" ? "Reading and fingerprinting eligible files…" : busy === "folders" ? "Reading current application folders…" : busy ? "Working on the selected batch…" : message}</p>}
    {busy === "plan" && <button type="button" className="quiet-button" onClick={() => void api.stopCleanup().catch((e: unknown) => setError(String(e)))}>Stop preparing plan</button>}
    <div className="overview-columns">
      {rows ? <StorageSummary rows={rows}/> : <div className="card"><Empty icon="drive" title={error ? "Cleanup totals unavailable" : "Loading storage summary…"}/></div>}
      <section className="card tool-card" aria-label="Prepare quarantine">
        <CardHeading title="Prepare quarantine" icon="archive" tone="violet" subtitle="A completed session, a Windows registration, and approved application folders"/>
        <div className="stack">
          <div className="field"><label htmlFor="cleanup-session">Installation</label><select id="cleanup-session" value={sessionId ?? ""} disabled={locked} onChange={(e) => { setSessionId(e.target.value ? Number(e.target.value) : null); setLink(false); }}>
            <option value="">Select a completed installation</option>
            {sessions.filter((s) => s.status === "completed").map((s) => <option key={s.session_id} value={s.session_id}>{s.app_name} (#{s.session_id})</option>)}
          </select></div>
          <div className="field"><label htmlFor="cleanup-registration">Registered application</label><select id="cleanup-registration" value={programId} disabled={locked} onChange={(e) => { setProgramId(e.target.value); setFolderChoices(null); setFolders([]); setLink(false); }}>
            <option value="">Select a Windows registration</option>
            {programs.map((p) => <option key={p.program.id} value={p.program.id}>{p.program.name} · {p.program.version ?? "unknown version"} · {p.program.id}{p.removed ? " (recently removed)" : ""}</option>)}
          </select></div>
          {!!registration && sessionId !== null && !linked && !registration.removed && <label className="check"><input type="checkbox" checked={link} disabled={locked} onChange={(e) => setLink(e.target.checked)}/> Link this recorded session to the selected registration</label>}
          {!!registration && linked && <small>Explicit application link on record.</small>}
          <PolicyChoice value={override} onChange={setOverride} disabled={locked}/>
          {folderChoices && <fieldset disabled={locked}><legend>Approved folders</legend>
            {folderChoices.map((choice) => <label key={choice.path} className="check">
              <input type="checkbox" disabled={!choice.can_quarantine || (!folders.includes(choice.path) && folders.length >= 8)} checked={folders.includes(choice.path)} onChange={(e) => setFolders((old) => e.target.checked ? [...old, choice.path] : old.filter((x) => x !== choice.path))}/>
              <span><code>{choice.path}</code><small>{choice.reason}{!choice.can_quarantine && " Review only."}</small></span>
            </label>)}
            {!folderChoices.length && <p>No eligible application folders found.</p>}
          </fieldset>}
          <div className="button-row">
            <button type="button" className="quiet-button" onClick={onSimulate} disabled={locked}>Open policy simulator</button>
            <button type="button" className="quiet-button" disabled={locked || !programId} onClick={() => void work("folders", async () => { setFolders([]); setFolderChoices(await api.cleanupFolders(programId)); })}>Find application folders</button>
            <button type="button" className="primary" disabled={locked || sessionId === null || !folders.length || !(linked || link)} onClick={(e) => {
              if (sessionId === null) return;
              reviewTrigger.current = e.currentTarget;
              void work("plan", async () => {
                if (!linked && link) {
                  await api.linkCleanupSession(sessionId, programId);
                  setPrograms((list) => list.map((p) => p.program.id === programId ? { ...p, session_ids: [...p.session_ids, sessionId] } : { ...p, session_ids: p.session_ids.filter((id) => id !== sessionId) }));
                  setLink(false);
                }
                setPlan(await api.prepareCleanup({ programId, sessionId, template: override ?? defaultPolicy, folders }));
              });
            }}>Review cleanup plan…</button>
          </div>
        </div>
      </section>
    </div>
    {plan && <CleanupPlanReview key={plan.id} plan={plan} onClose={() => setPlan(null)} onComplete={(out) => { setPlan(null); setMessage(out.summary); setOffset(0); refresh((n) => n + 1); }}/>}
    <section className="card card-overflow" id="batch-list-title" tabIndex={-1} aria-label="Quarantine batches">
      <CardHeading title="Quarantine batches" subtitle="Files stay on disk until permanently deleted or rotated" action={rows && <Badge>{rows.length} on page</Badge>}/>
      {rows?.length ? <div className="dashboard-scroll"><table className="dashboard-table batch-table">
        <thead><tr><th scope="col">Batch</th><th scope="col">Installation</th><th scope="col" className="numeric">Files</th><th scope="col" className="numeric">Recorded size</th><th scope="col">Status</th><th scope="col">Actions</th></tr></thead>
        <tbody>{rows.map((r) => <tr key={r.quarantine_id}>
          <td><code>{r.quarantine_id}</code><button className="text-button batch-view" type="button" disabled={locked} onClick={(e) => openBatch(r.quarantine_id, false, e.currentTarget)}>View batch</button></td><td>{r.session_id === null ? "Unassigned" : sessions.find((s) => s.session_id === r.session_id)?.app_name ?? `#${r.session_id}`}</td>
          <td className="numeric">{r.file_bytes == null && !r.files ? "—" : r.files}</td><td className="numeric">{formatBytes(r.file_bytes)}</td>
          <td><Badge tone={states[r.status]?.tone} dot>{states[r.status]?.label ?? r.status}</Badge></td>
          <td><div className="button-row">
            {(r.status === "quarantined" || r.status === "restoring") && <button type="button" className="quiet-button" disabled={locked} onClick={(e) => openBatch(r.quarantine_id, true, e.currentTarget)}>{r.status === "restoring" ? "Review interrupted restore…" : "Restore…"}</button>}
            {(r.status === "quarantined" || r.status === "purging") && <button type="button" className="text-button danger-text" disabled={locked} onClick={(e) => confirmBatch("purge", r.quarantine_id, e.currentTarget)}>{r.status === "purging" ? "Review interrupted delete…" : "Delete permanently…"}</button>}
            {r.status === "quarantined" && <button type="button" className="text-button" disabled={locked} onClick={(e) => confirmBatch("rotate", r.quarantine_id, e.currentTarget)}>Rotate verified archive…</button>}
          </div></td>
        </tr>)}</tbody>
      </table></div> : <Empty icon="archive" title={rows ? "Nothing has been quarantined yet" : error ? "Batches unavailable" : "Loading batches…"}/>}
      {(offset > 0 || page?.next_offset != null) && <div className="pager">
        <button type="button" disabled={locked || offset === 0} onClick={() => { setReview(null); setOffset(Math.max(0, offset - 40)); }}>Newer batches</button>
        <span>Page {offset / 40 + 1}</span><button type="button" disabled={locked || page?.next_offset == null} onClick={() => { setReview(null); setOffset(page?.next_offset ?? 0); }}>Older batches</button>
      </div>}
    </section>
    {confirm && <dialog ref={confirmDialog} className="wizard tool-card" aria-labelledby="batch-action-title" onCancel={(e) => { e.preventDefault(); setConfirm(null); }}>
      <h2 id="batch-action-title">{confirm.kind === "purge" ? "Confirm permanent delete" : "Rotate a verified archive"}</h2>
      <p>Batch <code>{confirm.id}</code>. {confirm.kind === "purge" ? "This deletes the held files locally and cannot be undone here. History remains." : "Recheck this exact batch in a complete profile backup before deleting local held files. Keep that backup to recover the archived bytes; History remains."}</p>
      {confirm.kind === "rotate" && <div className="stack"><label>Verified backup folder<input value={archive} onChange={(e) => setArchive(e.target.value)} maxLength={4096}/></label><BrowsePath directory label="Browse backup folder…" onPick={setArchive}/></div>}
      <div className="button-row"><button ref={cancelConfirm} type="button" className="quiet-button" onClick={() => setConfirm(null)}>Cancel</button>
        <button type="button" className="danger-button" disabled={confirm.kind === "rotate" && !archive.trim()} onClick={() => {
          const { kind, id } = confirm;
          act(async () => kind === "purge" ? `Deleted ${await api.purgeQuarantine(id)} item(s) permanently.` : `Verified the backup and removed ${await api.rotateArchivedBatch(id, archive)} local held item(s).`);
        }}>{confirm.kind === "purge" ? "Confirm: delete" : "Verify archive and rotate"}</button></div>
    </dialog>}
    {visible && reviewing && review && <BatchReview key={review.id} row={reviewing}
      state={states[reviewing.status] ?? { label: reviewing.status, tone: "neutral" }} reviewRestore={review.restore}
      onClose={() => setReview(null)} onReviewRestore={() => { if (!locked) setReview({ id: review.id, restore: true }); }}
      onRestore={() => { if (review.restore && !confirm && (reviewing.status === "quarantined" || reviewing.status === "restoring")) {
        act(async () => `Restored ${await api.restoreQuarantine(review.id)} item(s).`);
      } }}/>}
  </section>;
}
