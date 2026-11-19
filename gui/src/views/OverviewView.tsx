import { useEffect, useState } from "react";
import { api } from "../api";
import { ChangeBadge, changeStyle, domainIcon } from "../components/ChangeEvidence";
import { Icon } from "../components/Icon";
import { StorageSummary, summarizeStorage } from "../components/StorageSummary";
import { AppMark, Badge, CardHeading, Empty, Intro, Metric, TileIcon, countText, formatBytes, leafName, programSize, sourceText, timeText } from "../components/Workspace";
import type { HistoryRow, ProgramInventory, QuarantineRow, Session } from "../types";

type Result = { inventory: ProgramInventory | null; sessions: Session[] | null; batches: QuarantineRow[] | null; recent: HistoryRow[] | null; errors: string[] };

export function OverviewView({ visible, onApplications, onWatch, onHistory, onCleanup }: {
  visible: boolean;
  onApplications: (programId?: string) => void;
  onWatch: () => void;
  onHistory: (row?: HistoryRow) => void;
  onCleanup: () => void;
}) {
  const [data, setData] = useState<Result | null>(null);
  const [revision, refresh] = useState(0);
  const [pending, setPending] = useState(true);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    setPending(true);
    void Promise.allSettled([api.listPrograms(false), api.listSessions(), api.listQuarantines(), api.history(null, 5, null, null)] as const)
      .then(([i, s, b, h]) => {
        if (!live) return;
        setData({
          inventory: i.status === "fulfilled" ? i.value : null, sessions: s.status === "fulfilled" ? s.value : null,
          batches: b.status === "fulfilled" ? b.value : null, recent: h.status === "fulfilled" ? h.value.rows : null,
          errors: [i, s, b, h].flatMap((r) => r.status === "rejected" ? [String(r.reason)] : []),
        });
        setPending(false);
      });
    return () => { live = false; };
  }, [revision, visible]);

  const programs = data?.inventory?.programs.filter((row) => !row.removed).map(({ program }) => program) ?? [];
  const newest = [...programs].sort((a, b) => (b.install_date ?? "").localeCompare(a.install_date ?? "") || a.name.localeCompare(b.name)).slice(0, 5);
  const snapshots = data?.sessions?.filter((s) => s.status === "completed").length;
  const storage = data?.batches && summarizeStorage(data.batches);
  return <section className="workspace-page overview-page" aria-labelledby="overview-title">
    <Intro id="overview-title" kicker="SYSTEM WORKSPACE" title="System overview" description="Review installed apps, trace system changes, and plan cleanups from one place."
      actions={<>
        <button className="quiet-button" type="button" disabled={pending} onClick={() => refresh((v) => v + 1)}><Icon name="refresh"/> Refresh</button>
        <button className="primary" type="button" onClick={onWatch}><Icon name="plus"/> Watch an install</button>
      </>}/>
    {data?.errors.length ? <div className="notice review-notice" role="alert"><Icon name="alert"/><div><strong>Some dashboard data is unavailable.</strong> {data.errors.join(" · ")}</div></div> : null}
    {!!data?.inventory?.warnings.length && <details className="notice review-notice"><summary>Inventory is partial · {data.inventory.warnings.length} warning(s)</summary><ul>{data.inventory.warnings.map((w) => <li key={w}>{w}</li>)}</ul></details>}
    <div className="metrics" aria-label="Workspace metrics" aria-busy={pending}>
      <Metric icon="apps" tone="blue" label="Installed applications" value={data?.inventory ? countText(programs.length) : "—"}
        detail={data?.inventory ? `${programs.filter((p) => p.source === "registry").length} classic · ${programs.filter((p) => p.source === "msix").length} MSIX · ${programs.filter((p) => p.source === "portable").length} portable` : "Windows and portable inventory"}/>
      <Metric icon="pulse" tone="violet" label="Recorded sessions" value={data?.sessions ? countText(data.sessions.length) : "—"}
        detail={snapshots === undefined ? "Before/after observations" : `${snapshots} completed · ${data?.sessions?.filter((s) => s.status === "monitoring").length ?? 0} open`}/>
      <Metric icon="archive" tone="review" label="In quarantine" value={storage ? formatBytes(storage.held) : "—"}
        detail={storage ? `${storage.heldBatches} batch${storage.heldBatches === 1 ? "" : "es"} held in the latest 40 batches` : "Reversible file batches"}/>
      <Metric icon="drive" tone="safe" label="Deleted in cleanup" value={storage ? formatBytes(storage.purged) : "—"} detail="Recorded sizes in the latest 40 batches"/>
    </div>
    <div className="overview-columns">
      <section className="card card-overflow" aria-label="Installed applications">
        <CardHeading title="Installed applications" subtitle="From your Windows inventory" action={<button type="button" className="text-button" onClick={() => onApplications()}>View inventory <Icon name="arrow" size={15}/></button>}/>
        {newest.length ? <div className="dashboard-scroll"><table className="dashboard-table overview-app-table">
          <thead><tr><th scope="col">Application</th><th scope="col">Type</th><th scope="col" className="numeric">Reported size</th><th scope="col"><span className="sr-only">Open</span></th></tr></thead>
          <tbody>{newest.map((p) => <tr key={p.id} className="interactive-row" onClick={() => onApplications(p.id)}>
            <td><button className="app-cell row-button" type="button"><AppMark program={p}/><span><strong>{p.name}</strong><small>{[p.publisher, p.version].filter(Boolean).join(" · ") || "Version not reported"}</small></span></button></td>
            <td><Badge>{sourceText[p.source]}</Badge></td><td className="numeric">{programSize(p.size_kb)}</td><td aria-hidden="true"><Icon name="chevron" size={14}/></td>
          </tr>)}</tbody>
        </table></div> : <Empty icon="apps" title={pending ? "Loading inventory…" : "No applications to show"} detail="Browse the inventory to see Windows and portable entries."/>}
      </section>
      <section className="card card-overflow" aria-label="Recent changes">
        <CardHeading title="Recent changes" subtitle="Latest recorded evidence" action={<button type="button" className="text-button" onClick={() => onHistory()}>Open inspector <Icon name="arrow" size={15}/></button>}/>
        {data?.recent?.length ? <ul className="activity-list">{data.recent.map((row) => <li key={row.id}>
          <button type="button" className="activity-row row-button" onClick={() => onHistory(row)} title={row.path}>
            <TileIcon icon={domainIcon[row.domain] ?? "file"} tone={changeStyle(row.what).tone}/>
            <span className="activity-path"><strong>{leafName(row.path)}</strong><small>{row.path}</small></span>
            <span className="activity-meta"><ChangeBadge what={row.what} signed/><small>{timeText(row.when)}</small></span>
          </button>
        </li>)}</ul> : <Empty icon="changes" title={pending ? "Loading recorded changes…" : "No changes recorded yet"} detail="Take two snapshots with Watch an install to see what changed."/>}
      </section>
    </div>
    <div className="overview-columns overview-bottom">
      {data?.batches ? <StorageSummary rows={data.batches} action={<button className="text-button" type="button" onClick={onCleanup}>Cleanup summary <Icon name="arrow" size={15}/></button>}/>
        : <div className="card"><Empty icon="drive" title={pending ? "Loading storage summary…" : "Cleanup totals unavailable"}/></div>}
      <section className="card guard-card" aria-label="Cleanup boundaries">
        <CardHeading title="Review before removing" icon="shield" subtitle="Files can be restored from quarantine."/>
        <ul><li><Icon name="check" size={16}/> Protected and shared items stay put in policy cleanup.</li>
          <li><Icon name="check" size={16}/> Registry keys, services, and tasks are advisory.</li>
          <li><Icon name="check" size={16}/> Uninstallers run only after you confirm.</li></ul>
      </section>
    </div>
  </section>;
}
