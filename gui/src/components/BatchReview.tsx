import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../api";
import type { BatchPreview, QuarantineRow } from "../types";
import { Badge, formatBytes, type Tone } from "./Workspace";

export function BatchReview({ row, state, reviewRestore, onClose, onReviewRestore, onRestore }: {
  row: QuarantineRow; state: { label: string; tone: Tone };
  reviewRestore: boolean; onClose: () => void; onReviewRestore: () => void; onRestore: () => void;
}) {
  const [offsets, setOffsets] = useState([0]);
  const offset = offsets.at(-1) ?? 0;
  const [attempt, setAttempt] = useState(0);
  const key = `${offset}:${attempt}:${row.status}`;
  const [result, setResult] = useState<{ key: string; data?: BatchPreview; error?: string } | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  useLayoutEffect(() => { heading.current?.focus(); }, [reviewRestore]);
  useEffect(() => {
    let live = true;
    api.quarantineBatch(row.quarantine_id, offset).then((data) => { if (live) setResult({ key, data }); })
      .catch((e: unknown) => { if (live) setResult({ key, error: String(e) }); });
    return () => { live = false; };
  }, [key, row.quarantine_id, offset]);
  const current = result?.key === key ? result : null;
  const batch = current?.data;
  const created = batch ? new Date(batch.created_unix_ms) : null;
  const nextOffset = batch?.next_offset ?? null;
  const held = row.status === "quarantined" && batch?.status === "quarantined" && batch.total_files > 0;

  return <section className="card tool-card batch-review" aria-labelledby="batch-review-title" onKeyDown={(event) => {
    if (event.key === "Escape" && !event.defaultPrevented) { event.stopPropagation(); onClose(); }
  }}>
    <div className="batch-review-heading">
      <div><div className="eyebrow">{reviewRestore ? "RESTORE REVIEW" : "RECORDED MANIFEST"}</div>
        <h2 id="batch-review-title" tabIndex={-1} ref={heading}>{reviewRestore ? "Review restore" : "Batch preview"}</h2></div>
      <button className="quiet-button" type="button" onClick={onClose}>{reviewRestore ? "Cancel restore" : "Close batch preview"}</button>
    </div>
    <p className="batch-review-meta"><code>{row.quarantine_id}</code> <Badge tone={state.tone}>{state.label}</Badge>
      {created && Number.isFinite(created.valueOf()) && <span className="muted">Created <time dateTime={created.toISOString()}>{created.toLocaleString()}</time></span>}</p>
    <p className="muted">These are the original paths and sizes recorded when the files were moved. This preview doesn’t check the destinations or open held files.</p>
    {current?.error ? <div><p className="error" role="alert">Could not read the recorded batch: {current.error}</p><button type="button" onClick={() => setAttempt((n) => n + 1)}>Retry batch preview</button></div>
      : !batch ? <p role="status">Reading the recorded manifest…</p> : batch.files.length ? <>
        <p role="status" className="muted">Files {offset + 1}–{offset + batch.files.length} of {batch.total_files} recorded</p>
        <ul className="artifact-list batch-files" aria-label="Original file paths">{batch.files.map((file) => <li key={file.original_path}><code>{file.original_path}</code><span>{formatBytes(file.size)}</span></li>)}</ul>
      </> : <p role="status">No file paths recorded on this page.</p>}
    {(offset > 0 || nextOffset !== null || (result?.data?.total_files ?? 0) > 24) && <div className="pager batch-pager">
      <button type="button" aria-disabled={offset === 0} onClick={() => { if (offset > 0) setOffsets((list) => list.slice(0, -1)); }}>Previous files</button>
      <span aria-live="polite">Page {offsets.length}</span>
      <button type="button" aria-disabled={nextOffset === null} onClick={() => { if (nextOffset !== null) setOffsets((list) => [...list, nextOffset]); }}>Next files</button>
    </div>}
    {reviewRestore ? <div role="group" aria-label="Confirm restore" className="notice review-notice confirm-box">
      <div><strong>{held ? `Restore all ${batch.total_files} files to their original paths?` : batch ? "Only files in a held batch can be restored." : "Read the batch record before confirming."}</strong>
        <p>ExorSystem checks the batch and destinations again on restore. It never overwrites existing files or applies registry changes.</p></div>
      <div className="button-row"><button className="primary" type="button" disabled={!held} onClick={onRestore}>Confirm: restore files</button></div>
    </div> : held && <button className="quiet-button" type="button" onClick={onReviewRestore}>Review restore…</button>}
  </section>;
}
