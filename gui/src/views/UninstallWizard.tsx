import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { CleanupPlanReview } from "../components/CleanupPlanReview";
import { Icon } from "../components/Icon";
import { TerminalPanel } from "../components/TerminalPanel";
import { Badge, Empty, nodeText } from "../components/Workspace";
import type { InstalledProgram, LeftoverReview, PlanHeader, QuarantineResult, UninstallPreview, UninstallRun } from "../types";

const steps = ["Review plan", "Vendor uninstall", "Review leftovers", "Summary"];
type Task = "uninstall" | "review" | "quarantine";

export function UninstallWizard({ program, reviewOnly, onClose, onUninstallComplete }: {
  program: InstalledProgram; sessionId: number | null; reviewOnly: boolean; onClose: () => void; onUninstallComplete: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const gate = useRef(false);
  const [step, setStep] = useState(reviewOnly ? 2 : 0);
  const [skipVendor, setSkipVendor] = useState(reviewOnly);
  const [plan, setPlan] = useState<UninstallPreview | null>(null);
  const [planError, setPlanError] = useState("");
  const [busy, setBusy] = useState<Task | null>(null);
  const [confirmedRun, setConfirmedRun] = useState(false);
  const [trustUnverified, setTrustUnverified] = useState(false);
  const [movePlan, setMovePlan] = useState<PlanHeader | null>(null);
  const planTrigger = useRef<HTMLButtonElement>(null);
  const wasReviewing = useRef(false);
  useEffect(() => {
    if (movePlan) wasReviewing.current = true;
    else if (wasReviewing.current && step === 2) { planTrigger.current?.focus(); wasReviewing.current = false; }
  }, [movePlan, step]);
  const [review, setReview] = useState<LeftoverReview | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [run, setRun] = useState<UninstallRun | null>(null);
  const [move, setMove] = useState<QuarantineResult | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    const el = dialog.current;
    if (el && !el.open) el.showModal();
    return () => el?.close();
  }, []);
  useEffect(() => { heading.current?.focus(); }, [step]);
  useEffect(() => {
    if (reviewOnly) return;
    let live = true;
    api.previewUninstaller(program.id).then((p) => { if (live) setPlan(p); })
      .catch((e: unknown) => { if (live) setPlanError(String(e)); });
    return () => { live = false; };
  }, [program.id, reviewOnly]);

  const close = () => { if (!gate.current && !movePlan) { dialog.current?.close(); onClose(); } };
  const perform = async (task: Task, work: () => Promise<void>) => {
    if (gate.current) return;
    gate.current = true; setBusy(task); setError("");
    try { await work(); } catch (e) { setError(String(e)); }
    finally { gate.current = false; setBusy(null); }
  };
  const toReview = (skip = false) => {
    if (skip) setSkipVendor(true);
    setError(""); setStep(2);
  };

  return <dialog className="wizard" ref={dialog} aria-labelledby="wizard-title" onCancel={(e) => { e.preventDefault(); close(); }}>
    <div className="wizard-top">
      <span className="tile-icon tone-safe"><Icon name="shield"/></span>
      <div><div className="eyebrow">Guided uninstall</div><h2 id="wizard-title">{program.name}</h2></div>
      <button className="icon-button" type="button" aria-label="Close wizard" disabled={!!busy || !!movePlan} onClick={close}><Icon name="close"/></button>
    </div>
    <ol className="wizard-steps" aria-label="Wizard progress">
      {steps.map((label, i) => <li key={label} aria-current={i === step ? "step" : undefined} className={i === step ? "step-current" : i < step && (!skipVendor || i >= 2) ? "step-done" : ""}>
        <span className="step-number">{i < step && (!skipVendor || i >= 2) ? <Icon name="check" size={14}/> : `0${i + 1}`}</span>
        <span>{label}{skipVendor && i < 2 && <small>Skipped</small>}</span>
      </li>)}
    </ol>
    <div className="wizard-body">
      <div className="eyebrow">Step {step + 1} of {steps.length}</div>
      <h3 ref={heading} tabIndex={-1}>{steps[step]}</h3>
      {busy && <div className="notice busy-notice" role="status"><progress aria-label={busy === "uninstall" ? "Vendor uninstaller running" : "Working"}/><span>{busy === "uninstall" ? "Waiting for the vendor uninstaller. Windows does not report its progress here." : busy === "review" ? "Checking for possible leftovers…" : "Reading and fingerprinting eligible files…"}</span></div>}
      {error && <p role="alert" className="notice review-notice">{error}</p>}
      {step === 0 && <>
        <p>Review the live Windows registration first. The next step lets you start this vendor’s uninstaller. Cleanup is a separate decision.</p>
        <TerminalPanel preview={plan} message={planError ? "No validated invocation to show." : "Checking the live registration…"}/>
        {planError && <p className="notice review-notice" role="alert">{planError} Use Windows Settings → Apps for unsupported uninstallers.</p>}
        <div className="wizard-controls">
          <button type="button" className="quiet-button" onClick={() => toReview(true)}>Skip to leftover review</button>
          <button type="button" className="primary" disabled={!plan} onClick={() => setStep(1)}>Continue <Icon name="arrow" size={16}/></button>
        </div>
      </>}
      {step === 1 && <>
        {run ? <div className={`notice ${run.exit_code === 0 ? "success-notice" : "review-notice"}`} role="status">
          <Icon name={run.exit_code === 0 ? "check" : "alert"}/><div><strong>{run.exit_code === 0 ? "Vendor process exited" : "Uninstall not confirmed"}</strong><p>{run.summary}</p><small>Exit code: {run.exit_code ?? "unknown"}. Check Windows to confirm the app was removed.</small></div>
        </div> : <>
          <p>The vendor controls what its uninstaller removes. ExorSystem re-checks the live registration and refuses a changed invocation. It will not retry with administrator rights.</p>
          <label className="check confirm-check"><input type="checkbox" checked={confirmedRun} disabled={!!busy || plan?.signature.status === "rejected"} onChange={(e) => setConfirmedRun(e.target.checked)}/> Run this program’s own uninstaller</label>
          {plan && plan.signature.status === "rejected" && <div className="notice review-notice" role="alert"><Icon name="alert"/><div><strong>ExorSystem will not run this uninstaller</strong><p>{plan.signature.detail} Use Windows Settings → Apps, or reinstall the program first.</p></div></div>}
          {plan && (plan.signature.status === "unsigned" || plan.signature.status === "untrusted") && <>
            <div className="notice review-notice" role="status"><Icon name="alert"/><div><strong>Signature not verified</strong><p>{plan.signature.detail} Only continue if you trust where this program came from.</p></div></div>
            <label className="check confirm-check"><input type="checkbox" checked={trustUnverified} disabled={!!busy} onChange={(e) => setTrustUnverified(e.target.checked)}/> I trust this uninstaller even though its signature is not verified</label>
          </>}
        </>}
        <div className="wizard-controls">
          {!run && <button type="button" className="quiet-button" disabled={!!busy} onClick={() => { setError(""); setConfirmedRun(false); setTrustUnverified(false); setStep(0); }}><Icon name="back" size={15}/> Back</button>}
          {!busy && !run && error && <button type="button" className="quiet-button" onClick={() => toReview(true)}>Skip to leftover review</button>}
          {run ? <button type="button" className="primary" onClick={() => toReview()}>Continue to leftover review <Icon name="arrow" size={15}/></button> : <button type="button" className="primary" disabled={!!busy || !plan || !confirmedRun || plan.signature.status === "rejected" || (plan.signature.status !== "signed" && !trustUnverified)} onClick={() => {
            if (!plan) return;
            const trusted = plan.signature.status !== "signed" && trustUnverified;
            setConfirmedRun(false); setTrustUnverified(false);
            void perform("uninstall", async () => { setRun(await api.runUninstaller(program.id, plan, trusted)); onUninstallComplete(); });
          }}>Run uninstaller</button>}
        </div>
      </>}
      {step === 2 && <>
        <p>{skipVendor ? "Read-only review. This wizard has not run a vendor uninstaller. " : ""}Possible matches are not proof of ownership. Only files from explicitly selected folders can be moved; system entries are never changed.</p>
        {!review && <button type="button" className="quiet-button" disabled={!!busy} onClick={() => void perform("review", async () => setReview(await api.findLeftovers(program.id)))}><Icon name="search"/> Find leftovers</button>}
        {review && <>
          <p role="status" className="muted">{review.folders.length} possible folder(s) · {review.system_entries.length} advisory system item(s)</p>
          {!!review.warnings.length && <div className="notice review-notice"><Icon name="alert"/><ul>{review.warnings.map((w) => <li key={w}>{w}</li>)}</ul></div>}
          {!!review.folders.length && <fieldset className="leftover-folders">
            <legend>Leftover folders</legend>
            {review.folders.map((folder) => <div className="leftover-row" key={folder.path}>
              {folder.can_quarantine ? <label className="check">
                <input type="checkbox" checked={selected.has(folder.path)} disabled={!!busy || !!movePlan} onChange={(e) => {
                  const next = new Set(selected);
                  if (e.target.checked && next.size < 8) next.add(folder.path); else next.delete(folder.path);
                  setSelected(next);
                }}/><span><code>{folder.path}</code><small>{folder.reason}</small></span>
              </label> : <div><code>{folder.path}</code><small>{folder.reason} Outside an application root.</small></div>}
              <Badge tone={folder.can_quarantine ? "safe" : "review"}>{folder.can_quarantine ? "Eligible folder" : "Review only"}</Badge>
            </div>)}
          </fieldset>}
          {!!review.system_entries.length && <div className="wizard-system"><h4>System items (review only)</h4><ul className="review-list">{review.system_entries.map((item) => <li key={`${item.kind}:${item.path}`}>
            <Badge tone="review">{nodeText[item.kind]}</Badge> <code>{item.path}</code><small>{item.reason}</small></li>)}</ul></div>}
          {!review.folders.length && !review.system_entries.length && <Empty title="No matches found" detail="Uninspected locations are not treated as empty."/>}
        </>}
        {movePlan && <CleanupPlanReview key={movePlan.id} plan={movePlan} onClose={() => setMovePlan(null)} onComplete={(result) => { setMovePlan(null); setMove(result); setStep(3); }}/>}
        <div className="wizard-controls">
          {busy === "quarantine" && <button type="button" className="quiet-button" onClick={() => void api.stopCleanup().catch((e: unknown) => setError(String(e)))}>Stop preparing plan</button>}
          <button type="button" className="quiet-button" disabled={!!busy || !!movePlan} onClick={() => setStep(3)}>Skip quarantine</button>
          <button ref={planTrigger} type="button" className="primary" disabled={!!busy || !!movePlan || !selected.size} onClick={() => {
            void perform("quarantine", async () => setMovePlan(await api.prepareCleanup({ programId: program.id, folders: [...selected], sessionId: null, template: null })));
          }}>Review exact files…</button>
        </div>
      </>}
      {step === 3 && <>
        <div className="wizard-summary" role="status">
          <p><strong>Vendor uninstall:</strong> {run ? run.exit_code === 0 ? "Process exited with code 0." : `Not confirmed (exit ${run.exit_code ?? "unknown"}).` : "Not run."}</p>
          <p><strong>Leftover check:</strong> {review ? `${review.folders.length} possible folder(s) found.` : "Not run."}</p>
          <p><strong>Quarantine:</strong> {move?.summary ?? "No files moved by this wizard."}</p>
          {move && <p className="muted">Batch <code>{move.quarantine_id}</code> is available in Cleanup summary. Moved files are still held on disk.</p>}
        </div>
        <div className="wizard-controls"><button type="button" className="primary" onClick={close}>Done <Icon name="check" size={16}/></button></div>
      </>}
    </div>
  </dialog>;
}
