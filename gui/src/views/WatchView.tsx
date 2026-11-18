import { useEffect, useState } from "react";
import { api } from "../api";
import { ChangeBadge } from "../components/ChangeEvidence";
import { Icon } from "../components/Icon";
import { Badge, CardHeading, Empty, Intro, leafName } from "../components/Workspace";
import type { HistoryRow, WatchStart, WatchSummary } from "../types";

type Operation = "begin" | "finish" | "cancel";

function WatchWarnings({ warnings }: { warnings: string[] }) {
  return warnings.length ? <div className="notice review-notice"><Icon name="alert"/><ul>{warnings.map((w, i) => <li key={i}>{w}</li>)}</ul></div> : null;
}

export function WatchView({ hidden, onOpenHistory, onWatching }: { hidden: boolean; onOpenHistory: (id: number | null, row?: HistoryRow) => void; onWatching: (active: boolean) => void }) {
  const [appName, setAppName] = useState("");
  const [installer, setInstaller] = useState("");
  const [folders, setFolders] = useState("C:\\Program Files\nC:\\Program Files (x86)\nC:\\ProgramData");
  const [registrySubtrees, setRegistrySubtrees] = useState("");
  const [active, setActive] = useState<WatchStart | null>(null);
  const [pending, setPending] = useState<Operation | null>(null);
  const [error, setError] = useState("");
  const [summary, setSummary] = useState<WatchSummary | null>(null);
  const [recent, setRecent] = useState<HistoryRow[] | null>(null);
  const [recentError, setRecentError] = useState("");
  useEffect(() => { onWatching(!!active || !!pending); }, [active, pending, onWatching]);
  useEffect(() => {
    if (hidden) return;
    let live = true;
    setRecentError("");
    api.history(null, 5, null, null, "registry").then((page) => { if (live) setRecent(page.rows); })
      .catch((e: unknown) => { if (live) setRecentError(String(e)); });
    return () => { live = false; };
  }, [hidden, summary]);

  const run = async (operation: Operation, fn: () => Promise<void>) => {
    if (pending) return;
    setPending(operation); setError(""); setSummary(null);
    try { await fn(); } catch (e) {
      setError(operation === "finish" ? `Could not finish recording: ${String(e)}. Start a new watch to try again.` : String(e));
    } finally {
      if (operation !== "begin") setActive(null);
      setPending(null);
    }
  };
  const busy = pending !== null;
  const phase = summary ? 3 : pending === "finish" ? 2 : active ? 1 : 0;

  return <section className="workspace-page" aria-labelledby="h-watch" hidden={hidden}>
    <Intro id="h-watch" kicker="REGISTRY MONITOR" title="Watch an install" description="Take before and after snapshots of folders and selected registry keys to record changes."
      actions={<Badge tone={busy || active ? "safe" : "neutral"} dot>{pending ? "Working" : active ? "Baseline ready" : "Snapshot mode"}</Badge>}/>
    <div className="monitor-layout">
      <section className="card tool-card" aria-label="Snapshot controls">
        <CardHeading title="Snapshot monitor" icon="registry" tone="violet" subtitle="Observe without changing the registry"/>
        <ol className="monitor-steps" aria-label="Snapshot progress">{["Take baseline", "Run installer", "Compare snapshots"].map((label, i) => <li key={label} aria-current={i === phase ? "step" : undefined}><span>{i < phase ? <Icon name="check" size={15}/> : i + 1}</span>{label}</li>)}</ol>
        {!active && <form className="stack" onSubmit={(e) => {
          e.preventDefault();
          void run("begin", async () => { setActive(await api.beginWatch(appName, installer.trim() || null, folders.split("\n"), registrySubtrees)); });
        }}>
          <label htmlFor="w-app">Application name</label>
          <input id="w-app" value={appName} onChange={(e) => setAppName(e.target.value)} required disabled={busy} placeholder="e.g. Example Editor"/>
          <label htmlFor="w-inst">Installer file (optional)</label>
          <input id="w-inst" value={installer} onChange={(e) => setInstaller(e.target.value)} disabled={busy} placeholder="Full path to the installer"/>
          <label htmlFor="w-folders">Folders to watch, one per line</label>
          <textarea id="w-folders" rows={3} value={folders} onChange={(e) => setFolders(e.target.value)} disabled={busy}/>
          <label htmlFor="w-registry">Registry subtrees to watch (optional)</label>
          <small id="w-registry-hint" className="muted">Up to eight specific Software subtrees. Use <code>HKCU</code> or <code>HKLM</code>, and <code>HKLM32</code> for the 32-bit machine view. Leave blank to skip registry comparison.</small>
          <textarea id="w-registry" rows={3} maxLength={8192} aria-describedby="w-registry-hint" placeholder="HKCU\Software\Example" value={registrySubtrees} onChange={(e) => setRegistrySubtrees(e.target.value)} disabled={busy}/>
          <button className="primary" type="submit" disabled={busy}><Icon name="plus"/>{busy ? "Taking the first picture…" : "Take the first picture"}</button>
        </form>}
        {active && <>
          <div className="notice success-notice" role="status"><Icon name={busy ? "clock" : "check"}/><span>{busy ? (pending === "cancel" ? "Stopping the watch…" : "Taking the after picture…") : `Ready to watch ${appName} (session #${active.session_id}). Run the installer yourself, then return to finish. You can switch views while the watch is open.`}</span></div>
          <WatchWarnings warnings={active.warnings}/>
          <div className="button-row"><button className="primary" type="button" disabled={busy} onClick={() => void run("finish", async () => { setSummary(await api.finishWatch()); })}>Finish: take the second picture</button><button className="quiet-button" type="button" disabled={busy} onClick={() => void run("cancel", api.cancelWatch)}>Cancel</button></div>
        </>}
        {error && <p role="alert" className="notice review-notice">{error}</p>}
        {summary && <div className="watch-result">
          <div className="notice success-notice" role="status"><Icon name="check"/><div><strong>{summary.summary}</strong><p>{summary.registry_scopes_compared ? `Registry key changes: ${summary.registry_keys_changed} across ${summary.registry_scopes_compared} compared subtree(s).` : "No registry subtrees were compared."}</p></div></div>
          <WatchWarnings warnings={summary.warnings}/><button type="button" className="primary" onClick={() => onOpenHistory(summary.session_id)}>View recorded changes (session #{summary.session_id}) <Icon name="arrow" size={15}/></button>
        </div>}
      </section>
      <div className="monitor-side">
        <section className="card card-overflow" aria-label="Recent registry observations"><CardHeading title="Registry observations" subtitle="Latest recorded key changes"/>
          {recentError ? <p className="error tool-card" role="alert">Could not load registry observations: {recentError}</p> : recent?.length ? <ul className="diff-list">{recent.map((r) => <li key={r.id}><button className="row-button evidence-row" type="button" title={r.path} onClick={() => onOpenHistory(r.session_id, r)}><ChangeBadge what={r.what} signed/><span><strong>{leafName(r.path)}</strong><code>{r.path}</code></span><Icon name="chevron" size={15}/></button></li>)}</ul>
            : <Empty icon="registry" title={recent ? "No key changes recorded" : "Loading observations…"} detail={recent ? "Add a registry subtree to a watch, then compare the snapshots." : undefined}/>}
        </section>
        <div className="notice monitor-tip"><Icon name="lock"/><div><strong>Snapshot evidence only</strong><p>Key names and last-write changes are checked. No values are collected, and no process is attributed to a change. Unreadable scopes are skipped, not treated as empty.</p></div></div>
      </div>
    </div>
  </section>;
}
