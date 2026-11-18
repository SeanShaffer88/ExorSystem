import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "../components/Icon";
import { TerminalPanel } from "../components/TerminalPanel";
import { CardHeading, Intro } from "../components/Workspace";
import type { InstalledProgram, UninstallPreview } from "../types";

export const hasVendorCommand = (p: InstalledProgram) => p.source === "registry" && p.scope !== "user" && !!p.uninstall_command;

export function TerminalView({ initialProgramId, onInventory }: { initialProgramId: string | null; onInventory: () => void }) {
  const [programs, setPrograms] = useState<InstalledProgram[] | null>(null);
  const [id, setId] = useState(initialProgramId ?? "");
  const [request, setRequest] = useState<{ id: string } | null>(initialProgramId ? { id: initialProgramId } : null);
  const [result, setResult] = useState<{ request: NonNullable<typeof request>; plan?: UninstallPreview; error?: string } | null>(null);
  const [inventoryError, setInventoryError] = useState("");
  const current = result && result.request === request ? result : null;
  const loading = !!request && !current;

  useEffect(() => {
    let live = true;
    api.listPrograms(false).then((data) => {
      if (live) setPrograms(data.programs.filter((r) => !r.removed && hasVendorCommand(r.program)).map((r) => r.program));
    }).catch((e: unknown) => { if (live) setInventoryError(String(e)); });
    return () => { live = false; };
  }, []);

  useEffect(() => {
    if (!request) return;
    let live = true;
    api.previewUninstaller(request.id).then((plan) => { if (live) setResult({ request, plan }); })
      .catch((e: unknown) => { if (live) setResult({ request, error: String(e) }); });
    return () => { live = false; };
  }, [request]);

  return <section className="workspace-page" aria-labelledby="preview-title">
    <Intro id="preview-title" kicker="ANALYSIS" title="Script preview" description="Inspect the executable and arguments a vendor registered, without running anything."/>
    <div className="card tool-card">
      <CardHeading title="Choose an invocation" icon="terminal" subtitle="Supported machine-wide classic applications only"/>
      <form className="tool-form" onSubmit={(e) => { e.preventDefault(); if (id) setRequest({ id }); }}>
        <label htmlFor="terminal-app">Application</label>
        <select id="terminal-app" value={id} onChange={(e) => { setId(e.target.value); setRequest(null); }}>
          <option value="">Select an application</option>
          {!!id && !programs?.some((p) => p.id === id) && <option value={id}>Selected inventory entry</option>}
          {programs?.map((p) => <option key={p.id} value={p.id}>{p.name}{p.version ? ` · ${p.version}` : ""}</option>)}
        </select>
        <button className="primary" type="submit" disabled={!id || loading}><Icon name="search"/> Load preview</button>
      </form>
      {programs?.length === 0 && <p className="muted">No applications with a machine-wide uninstall registration are listed.</p>}
      {inventoryError && <p role="alert" className="error">Could not load inventory: {inventoryError}</p>}
    </div>
    {current?.error && <p role="alert" className="notice review-notice">{current.error} Use Windows Settings → Apps if the registration is unsupported.</p>}
    <TerminalPanel preview={current?.plan ?? null} message={loading ? "Checking the live registration…" : current?.error ? "No validated invocation to show." : undefined}/>
    <div className="notice"><Icon name="lock"/><div><strong>Read-only terminal.</strong> There is no command input. To launch a supported vendor uninstaller, open its wizard from Applications and confirm the reviewed invocation.</div>
      <button type="button" className="text-button" onClick={onInventory}>Applications <Icon name="arrow" size={15}/></button></div>
  </section>;
}
