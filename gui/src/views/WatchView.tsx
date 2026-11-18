import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { ChangeBadge } from "../components/ChangeEvidence";
import { Icon } from "../components/Icon";
import { RecentWatches } from "../components/RecentWatches";
import { WatchPresets } from "../components/WatchPresets";
import { comparisonLabel, hasSkipped, hasUsable, WatchCoverageCards, WatchWarnings } from "../components/WatchCoverage";
import { Badge, CardHeading, Empty, Intro, leafName } from "../components/Workspace";
import type { HistoryRow, WatchStart, WatchSummary, WatchSystem } from "../types";

type Operation = "begin" | "finish" | "cancel";
let nextCaptureId = 0;

export function WatchView({ hidden, onOpenHistory, onWatching }: { hidden: boolean; onOpenHistory: (id: number | null, row?: HistoryRow) => void; onWatching: (active: boolean) => void }) {
  const [appName, setAppName] = useState("");
  const [installer, setInstaller] = useState("");
  const [folders, setFolders] = useState("");
  const [registrySubtrees, setRegistrySubtrees] = useState("");
  const [system, setSystem] = useState<WatchSystem>({ services: false, tasks: false });
  const [active, setActive] = useState<WatchStart | null>(null);
  const [pending, setPending] = useState<Operation | null>(null);
  const [error, setError] = useState("");
  const [cancelled, setCancelled] = useState("");
  const [stopPending, setStopPending] = useState(false);
  const [stopNote, setStopNote] = useState("");
  const [captureReady, setCaptureReady] = useState(false);
  const running = useRef<number | null>(null);
  const [summary, setSummary] = useState<WatchSummary | null>(null);
  const [recent, setRecent] = useState<HistoryRow[] | null>(null);
  const [recentError, setRecentError] = useState("");
  const controls = useRef<HTMLElement>(null);
  const status = useRef<HTMLDivElement>(null);
  const appField = useRef<HTMLInputElement>(null);
  useEffect(() => { onWatching(!!active || !!pending); }, [active, pending, onWatching]);
  useEffect(() => {
    if (hidden) return;
    let live = true;
    setRecentError("");
    api.history(null, 5, null, null, "registry").then((page) => { if (live) setRecent(page.rows); })
      .catch((e: unknown) => { if (live) setRecentError(String(e)); });
    return () => { live = false; };
  }, [hidden, summary]);
  useEffect(() => {
    // The previous step's button may have unmounted. Don't steal focus from another view.
    if (!hidden && !pending && (document.activeElement === document.body || controls.current?.contains(document.activeElement))) {
      (active || summary ? status.current : appField.current)?.focus();
    }
  }, [hidden, active, pending, summary]);

  const run = async (operation: Operation, fn: (captureId: number, ready: () => void) => Promise<void>) => {
    if (running.current !== null) return;
    const id = ++nextCaptureId;
    running.current = id;
    setPending(operation); setError(""); setCancelled(""); setStopNote("");
    try {
      await fn(id, () => { if (running.current === id) setCaptureReady(true); });
    } catch (e) {
      setError(operation === "finish" ? `Could not finish recording: ${String(e)}. Start a new watch to try again.` : String(e));
    } finally {
      if (operation !== "begin") setActive(null);
      running.current = null;
      setPending(null); setCaptureReady(false); setStopPending(false); setStopNote("");
    }
  };
  const stopCapture = async () => {
    const id = running.current;
    if (id === null || !captureReady || stopPending) return;
    setStopPending(true); setStopNote("Requesting a stop…");
    try {
      const stopping = await api.stopWatchCapture(id);
      if (running.current !== id) return;
      setStopPending(stopping);
      setStopNote(stopping ? "Stopping after the current read…" : "No stop was requested. Try again if the capture is still running.");
    } catch (e) {
      if (running.current !== id) return;
      setStopPending(false); setStopNote(`Could not request a stop: ${String(e)}`);
    }
  };
  const stopButton = (pending === "begin" || pending === "finish") && <button className="quiet-button" type="button" disabled={!captureReady || stopPending} onClick={() => void stopCapture()}>Stop capture</button>;
  const busy = pending !== null;
  const canStart = !!appName.trim() && (!!folders.trim() || !!registrySubtrees.trim() || system.services || system.tasks);
  const compared = !!summary && hasUsable(summary.coverage);
  const partial = summary ? hasSkipped(summary.coverage) : !!active && hasSkipped(active.coverage);
  const comparisonStatus = summary ? comparisonLabel(summary.coverage) : "";
  const phase = summary ? (compared ? 3 : 2) : pending === "finish" ? 2 : active ? 1 : 0;
  const badge = pending ? "Working" : summary ? comparisonStatus : active ? (partial ? "Partial baseline" : "Baseline ready") : "Snapshot mode";

  return <section className="workspace-page" aria-labelledby="h-watch" hidden={hidden}>
    <Intro id="h-watch" kicker="REGISTRY MONITOR" title="Watch an install" description="Choose what to observe, then compare snapshots before and after your changes."
      actions={<Badge tone={busy ? "neutral" : partial ? "review" : active || summary ? "safe" : "neutral"} dot>{badge}</Badge>}/>
    <div className="monitor-layout">
      <section ref={controls} className="card tool-card" aria-label="Snapshot controls">
        <CardHeading title="Snapshot monitor" icon="registry" tone="violet" subtitle="Only the scopes you choose are checked"/>
        <ol className="monitor-steps" aria-label="Snapshot progress">{["Take baseline", "Make changes", "Compare snapshots"].map((label, i) => <li key={label} aria-current={i === phase ? "step" : undefined}><span>{i < phase ? <Icon name="check" size={15}/> : i + 1}</span>{label}</li>)}</ol>
        {!active && !summary && <div className="watch-setup">
          <WatchPresets disabled={busy} scopes={{ folders, registrySubtrees, system }} onApply={(scopes) => {
            setFolders(scopes.folders); setRegistrySubtrees(scopes.registrySubtrees); setSystem(scopes.system);
          }}/>
          <form className="stack watch-form" onSubmit={(e) => {
            e.preventDefault();
            if (canStart) void run("begin", async (id, ready) => {
              const roots = [...new Set(folders.split("\n").map((path) => path.trim()).filter(Boolean))];
              const start = await api.beginWatch({ appName: appName.trim(), installerPath: installer.trim() || null, folders: roots, registrySubtrees, system }, id, ready);
              if (start) setActive(start); else setCancelled("Baseline cancelled. No changes were recorded.");
            });
          }}>
            <div className="watch-identity">
              <div><label htmlFor="w-app">Application name</label>
                <input ref={appField} id="w-app" value={appName} onChange={(e) => setAppName(e.target.value)} required disabled={busy} placeholder="e.g. Example Editor"/></div>
              <div><label htmlFor="w-inst">Installer file (optional)</label>
                <input id="w-inst" value={installer} onChange={(e) => setInstaller(e.target.value)} disabled={busy} placeholder="Full path to the installer"/></div>
            </div>
            <label htmlFor="w-folders">Folders to watch (optional)</label>
            <small id="w-folders-hint" className="muted">Enter one full path per line. Start with an application or test folder rather than an entire drive.</small>
            <textarea id="w-folders" rows={2} aria-describedby="w-folders-hint" placeholder="C:\Program Files\Example" value={folders} onChange={(e) => setFolders(e.target.value)} disabled={busy}/>
            <label htmlFor="w-registry">Registry subtrees to watch (optional)</label>
            <small id="w-registry-hint" className="muted">Up to eight specific Software subtrees. Use <code>HKCU</code> or <code>HKLM</code>, and <code>HKLM32</code> for the 32-bit machine view. Leave blank to skip registry comparison.</small>
            <textarea id="w-registry" rows={2} maxLength={8192} aria-describedby="w-registry-hint" placeholder="HKCU\Software\Example" value={registrySubtrees} onChange={(e) => setRegistrySubtrees(e.target.value)} disabled={busy}/>
            <fieldset className="watch-options" disabled={busy}>
              <legend>Optional system checks</legend>
              <p id="w-system-hint" className="muted">Read-only metadata checks. Windows may restrict access to scheduled tasks; you do not need to run as Administrator.</p>
              <div className="button-row">
                <label className="check"><input type="checkbox" checked={system.services} onChange={(e) => setSystem({ ...system, services: e.target.checked })}/> Services</label>
                <label className="check"><input type="checkbox" checked={system.tasks} aria-describedby="w-system-hint" onChange={(e) => setSystem({ ...system, tasks: e.target.checked })}/> Scheduled tasks</label>
              </div>
            </fieldset>
            <div className="button-row watch-start-row">
              <button className="primary" type="submit" disabled={busy || !canStart}><Icon name={busy ? "clock" : "plus"}/>{busy ? "Taking baseline…" : "Take baseline"}</button>
              {stopButton}
              {busy ? <span className="muted" role="status">Reading selected scopes…</span> : <small>Choose at least one scope to start.</small>}
            </div>
          </form>
        </div>}
        {active && !summary && <div className="watch-result">
          <div ref={status} tabIndex={-1} className={`notice ${partial ? "review-notice" : "success-notice"}`} role="status">
            <Icon name={busy ? "clock" : partial ? "alert" : "check"}/><div>
              <strong>{busy ? (pending === "cancel" ? "Stopping the watch…" : "Comparing snapshots…") : `Ready to watch ${appName.trim()} (session #${active.session_id})`}</strong>
              {!busy && <p>{partial && "Some selected scopes could not be read. "}Make your test changes or run the installer yourself, then return to compare snapshots.</p>}
            </div>
          </div>
          <WatchCoverageCards coverage={active.coverage}/>
          <WatchWarnings warnings={active.warnings}/>
          <div className="button-row"><button className="primary" type="button" disabled={busy} onClick={() => void run("finish", async (id, ready) => {
            const result = await api.finishWatch(id, ready);
            if (result) setSummary(result); else setCancelled("Comparison cancelled. No changes were recorded. Take a new baseline to continue.");
          })}>Compare snapshots</button>{stopButton}<button className="quiet-button" type="button" disabled={busy} onClick={() => void run("cancel", api.cancelWatch)}>Cancel watch</button></div>
        </div>}
        {stopNote && <p role="status" className="muted">{stopNote}</p>}
        {cancelled && <p role="status" className="notice">{cancelled}</p>}
        {error && <p role="alert" className="notice review-notice">{error}</p>}
        {summary && <div className="watch-result">
          <div ref={status} tabIndex={-1} className={`notice ${partial ? "review-notice" : "success-notice"}`} role="status">
            <Icon name={partial ? "alert" : "check"}/><div>
              <strong>{comparisonStatus}</strong>
              <p>{compared ? `Session #${summary.session_id} · ${appName.trim()}. ${partial ? "The counts below cover only the scopes that could be compared." : "The counts below cover your selected scopes."}` : "No matching pairs of usable snapshots were available. Review the skipped scopes and try a new watch."}</p>
            </div>
          </div>
          <WatchCoverageCards coverage={summary.coverage} summary={summary}/>
          <WatchWarnings warnings={summary.warnings}/>
          <div className="button-row">
            <button type="button" className="primary" onClick={() => onOpenHistory(summary.session_id)}>{compared ? "View recorded changes" : "View watch coverage"} (session #{summary.session_id}) <Icon name="arrow" size={15}/></button>
            <button className="quiet-button" type="button" onClick={() => setSummary(null)}>Set up another watch</button>
          </div>
        </div>}
      </section>
      <div className="monitor-side">
        <RecentWatches visible={!hidden} comparedId={summary?.session_id} onInspect={onOpenHistory}/>
        <section className="card card-overflow" aria-label="Recent registry observations"><CardHeading title="Registry observations" subtitle="Latest recorded key changes"/>
          {recentError ? <p className="error tool-card" role="alert">Could not load registry observations: {recentError}</p> : recent?.length ? <ul className="diff-list">{recent.map((r) => <li key={r.id}><button className="row-button evidence-row" type="button" title={r.path} onClick={() => onOpenHistory(r.session_id, r)}><ChangeBadge what={r.what} signed/><span><strong>{leafName(r.path)}</strong><code>{r.path}</code></span><Icon name="chevron" size={15}/></button></li>)}</ul>
            : <Empty icon="registry" title={recent ? "No key changes recorded" : "Loading observations…"} detail={recent ? "Add a registry subtree to a watch, then compare the snapshots." : undefined}/>}
        </section>
        <div className="notice monitor-tip"><Icon name="lock"/><div><strong>Snapshot evidence only</strong><p>Snapshots show what changed, not which process changed it. No registry values are read. Unreadable scopes are skipped, not treated as empty. Coverage counts and skipped scope details are saved locally with the comparison.</p></div></div>
      </div>
    </div>
  </section>;
}
