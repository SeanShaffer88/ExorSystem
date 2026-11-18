import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { api } from "../api";
import type { PlanHeader, PlanPage, QuarantineResult } from "../types";
import { Badge, formatBytes } from "./Workspace";

export const REVIEW_PAGE = 40;
export function CleanupPlanReview({ plan, onClose, onComplete }: { plan: PlanHeader; onClose: () => void; onComplete: (result: QuarantineResult) => void }) {
  const title = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const gate = useRef(false);
  const [excluded, setExcluded] = useState(!plan.files);
  const [offset, setOffset] = useState(0);
  const [attempt, setAttempt] = useState(0);
  const [page, setPage] = useState<PlanPage | null>(null);
  const [seen, setSeen] = useState<Set<number>>(new Set());
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [invalid, setInvalid] = useState(false);
  const [error, setError] = useState("");
  const pages = Math.ceil(plan.files / REVIEW_PAGE);
  const reviewed = pages > 0 && seen.size === pages;

  useLayoutEffect(() => { heading.current?.focus(); }, []);
  useEffect(() => {
    let live = true;
    setPage(null); setError("");
    api.cleanupPlanPage(plan.id, offset, excluded).then((result) => {
      if (!live) return;
      setPage(result);
      if (!excluded && result.files.length) setSeen((previous) => new Set([...previous, result.offset]));
    }).catch((e: unknown) => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, [plan.id, offset, excluded, attempt]);

  const close = () => { if (!gate.current) { void api.discardCleanup(plan.id).catch(() => {}); onClose(); } };
  const apply = async () => {
    if (gate.current || invalid || !reviewed || !approved || !page) return;
    gate.current = true; setBusy(true); setError("");
    try { onComplete(await api.applyCleanup(plan.id, plan.files)); }
    catch (e) { setInvalid(true); setError(`${String(e)} Build a new plan before attempting another move.`); }
    finally { setBusy(false); gate.current = false; }
  };

  return <section className="card tool-card plan-review" aria-labelledby={title} onKeyDown={(e) => {
    if (e.key === "Escape" && !gate.current) { e.preventDefault(); e.stopPropagation(); close(); }
  }}>
    <h3 id={title} ref={heading} tabIndex={-1}>Review exact cleanup plan</h3>
    <div className="button-row"><Badge tone="violet">{plan.files} files · {formatBytes(plan.bytes)}</Badge><Badge tone="review">{plan.excluded} excluded</Badge></div>
    <p>Only the listed included files may move. The desktop rechecks their identity and the approved folders; changes require a new plan. This review expires in {plan.expires_in_seconds / 60} minutes.</p>
    <ul className="review-list" aria-label="Approved application folders">{plan.roots.map((root) => <li key={root}><code>{root}</code></li>)}</ul>
    <div className="segments" role="group" aria-label="Plan contents">
      <button type="button" aria-pressed={!excluded} disabled={busy} onClick={() => { setExcluded(false); setOffset(0); }}>Included files</button>
      <button type="button" aria-pressed={excluded} disabled={busy} onClick={() => { setExcluded(true); setOffset(0); }}>Exclusions</button>
    </div>
    {page && <><p className="muted">{page.total ? `${offset + 1}–${offset + page.files.length + page.exclusions.length} of ${page.total}` : "No items in this list."}</p>
      <ul className="review-list plan-files">{page.files.map((file) => <li key={file.path}><code>{file.path}</code><small>{formatBytes(file.size)}</small></li>)}
      {page.exclusions.map((item, index) => <li key={`${index}:${item.path}`}><code>{item.path}</code><small>{item.reason}</small></li>)}</ul>
    </>}
    {!page && !error && <p role="status">Loading review page…</p>}
    <div className="pager"><button type="button" disabled={busy || offset === 0} onClick={() => setOffset(Math.max(0, offset - REVIEW_PAGE))}>Previous paths</button>
      <span>Page {offset / REVIEW_PAGE + 1}</span><button type="button" disabled={busy || page?.next_offset == null} onClick={() => setOffset(page?.next_offset ?? 0)}>Next paths</button></div>
    {error && <p className="notice review-notice" role="alert">{error}</p>}
    {error && !invalid && <button type="button" onClick={() => setAttempt((n) => n + 1)}>Retry page</button>}
    {!!plan.files && <>
      <p className="muted">{seen.size} of {pages} included-file pages opened. Excluded items will not move.</p>
      <label className="check confirm-check"><input type="checkbox" checked={approved} disabled={!reviewed || busy || invalid} onChange={(e) => setApproved(e.target.checked)}/> I approve moving exactly these {plan.files} files to quarantine.</label>
    </>}
    <div className="button-row">
      <button type="button" className="quiet-button" disabled={busy} onClick={close}>Cancel review</button>
      <button type="button" className="primary" disabled={!approved || !reviewed || busy || invalid || !page} onClick={() => void apply()}>Confirm: move reviewed files</button>
    </div>
    {busy && <p role="status">Rechecking and moving the reviewed files. Closing waits for a safe boundary.</p>}
  </section>;
}
