import { useState } from "react";
import { api } from "../api";
import { getPolicy } from "../settings";
import { Icon } from "../components/Icon";
import type { InstalledProgram } from "../types";
import { hasVendorCommand } from "./TerminalView";
import { UninstallWizard } from "./UninstallWizard";

export function ProgramActions({ program, removed, sessionId, onUninstallComplete, onSimulate, onCleanup, onTerminal }: {
  program: InstalledProgram | null; removed: boolean; sessionId: number | null; onUninstallComplete: () => void;
  onSimulate: (id: number) => void; onCleanup: () => void; onTerminal: (id: string) => void;
}) {
  const [wizard, setWizard] = useState<"uninstall" | "review" | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const canLaunch = !!program && !removed && hasVendorCommand(program);
  const perform = async (fn: () => Promise<string>) => {
    if (busy) return;
    setBusy(true); setError(""); setNote("");
    try { setNote(await fn()); } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  return <div className="program-actions">
    {program?.source === "msix" && <p className="muted">Manage or remove this app in Windows Settings → Apps.</p>}
    {program?.source === "portable" && <p className="muted">Manage this inventory entry in Settings → Portable apps. Forgetting it leaves files untouched.</p>}
    {program?.source === "registry" && <>
      {removed && <p className="muted">Windows no longer lists this program. You can review leftovers for up to an hour in this run.</p>}
      {!removed && !canLaunch && <p className="muted">Use Windows Settings → Apps to uninstall this {program.scope === "user" ? "per-user " : ""}program. You can still review possible leftovers here.</p>}
      <div className="button-row">
        {canLaunch && <button type="button" className="primary" onClick={() => setWizard("uninstall")}><Icon name="shield"/> Uninstall wizard</button>}
        <button type="button" className="quiet-button" onClick={() => setWizard("review")}><Icon name="search"/> Review leftovers</button>
        {canLaunch && <button type="button" className="text-button" onClick={() => onTerminal(program.id)}><Icon name="terminal"/> Preview command</button>}
      </div>
      {wizard && <UninstallWizard program={program} mode={wizard === "review" ? "leftovers" : "uninstall"} onClose={() => setWizard(null)} onUninstallComplete={onUninstallComplete}/>}
    </>}
    {sessionId !== null && <div className="session-actions button-row">
      <button type="button" className="quiet-button" onClick={() => onSimulate(sessionId)}>Preview cleanup</button>
      <button type="button" className="quiet-button" onClick={onCleanup}>Cleanup summary</button>
      <button type="button" className="quiet-button" disabled={busy} onClick={() => void perform(async () => `Saved report to ${await api.exportReport(sessionId, getPolicy())}.`)}>Save report</button>
      <button type="button" className="quiet-button" disabled={busy} onClick={() => void perform(async () => (await api.makePortable(sessionId)).summary)}>Export recorded files and plan</button>
    </div>}
    {error && <p role="alert" className="error">{error}</p>}
    {note && <p role="status">{note}</p>}
  </div>;
}
