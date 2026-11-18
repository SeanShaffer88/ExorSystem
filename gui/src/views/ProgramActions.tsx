import { useState } from "react";
import { api } from "../api";
import { getPolicy } from "../settings";
import type { InstalledProgram, LeftoverReview } from "../types";

const reviewLabel = { registry_key: "Registry key", service: "Service", task: "Scheduled task" } as const;

interface Props {
  program: InstalledProgram | null;
  removed: boolean;
  sessionId: number | null;
  shared: number;
  onUninstallComplete: () => void;
}

/** State belongs to this program/session, so a late result cannot populate another program's pane. */
export function ProgramActions({ program, removed, sessionId, shared, onUninstallComplete }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [note, setNote] = useState("");
  const [confirmUninstall, setConfirmUninstall] = useState(false);
  const [confirmCleanup, setConfirmCleanup] = useState(false);
  const [review, setReview] = useState<LeftoverReview | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="pane pane-actions" aria-busy={busy}>
      <h2>Actions</h2>
      {program?.source === "msix" && <p>Manage or remove this app in Windows Settings → Apps. ExorSystem lists it for reference.</p>}
      {program?.source === "portable" && <p>Manage this inventory entry under Settings → Portable apps. Forgetting it does not remove any files.</p>}
      {program?.source === "registry" && (
        <>
          <h3>Uninstall and review</h3>
          {removed && <p>Windows no longer lists this program. You can review its leftovers until ExorSystem closes.</p>}
          {!removed && program.scope === "user" && <p>Use Windows Settings → Apps to uninstall this per-user program. You can still review its leftovers here.</p>}
          {!removed && program.scope !== "user" && program.uninstall_command && (
            <>
              <label className="check">
                <input type="checkbox" checked={confirmUninstall} disabled={busy} onChange={(e) => setConfirmUninstall(e.target.checked)} /> Run this program's own uninstaller
              </label>
              <button type="button" disabled={!confirmUninstall || busy} onClick={() => {
                setConfirmUninstall(false);
                setReview(null);
                void run(async () => {
                  const result = await api.runUninstaller(program.id, false);
                  setNote(result.summary);
                  onUninstallComplete();
                });
              }}>Uninstall</button>
            </>
          )}
          <button type="button" disabled={busy} onClick={() => {
            setReview(null);
            setSelected(new Set());
            void run(async () => setReview(await api.findLeftovers(program.id)));
          }}>Find leftovers</button>
          {review && (
            <>
              <p role="status">{review.folders.length} possible folder(s) and {review.system_entries.length} possible system item(s) found. Matches are not proof of ownership.</p>
              {review.warnings.length > 0 && (
                <div>
                  <h4>Some locations could not be checked</h4>
                  <ul className="review-list">{review.warnings.map((w) => <li key={w}>{w}</li>)}</ul>
                </div>
              )}
              {review.folders.length > 0 && (
                <fieldset className="leftover-folders">
                  <legend>Leftover folders</legend>
                  {review.folders.map((folder) => (
                    <div key={folder.path}>
                      {folder.can_quarantine ? (
                        <label className="check">
                          <input type="checkbox" checked={selected.has(folder.path)} disabled={busy} onChange={(e) => {
                            const checked = e.target.checked;
                            setSelected((previous) => {
                              const next = new Set(previous);
                              if (checked) next.add(folder.path);
                              else next.delete(folder.path);
                              return next;
                            });
                          }} />
                          <span>{folder.path} — {folder.reason}</span>
                        </label>
                      ) : <p>{folder.path} — {folder.reason} Outside an application root; review only.</p>}
                    </div>
                  ))}
                  {review.folders.some((folder) => folder.can_quarantine) && (
                    <button type="button" disabled={selected.size === 0 || busy} onClick={() => {
                      void run(async () => {
                        const result = await api.quarantineLeftovers(program.id, [...selected], sessionId);
                        setNote(result.summary);
                        setSelected(new Set());
                        setReview({ ...review, folders: review.folders.filter((f) => !selected.has(f.path)) });
                      });
                    }}>Quarantine files in selected folders</button>
                  )}
                </fieldset>
              )}
              {review.system_entries.length > 0 && (
                <div>
                  <h4>System items (review only)</h4>
                  <p>ExorSystem does not change registry keys, services or scheduled tasks.</p>
                  <ul className="review-list">
                    {review.system_entries.map((item) => (
                      <li key={`${item.kind}:${item.path}`}>
                        <strong>{reviewLabel[item.kind]}:</strong> {item.path}<br />{item.reason}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </>
          )}
        </>
      )}
      {sessionId !== null && (
        <>
          <h3>Recorded install</h3>
          <button type="button" disabled={busy} onClick={() => void run(async () => {
            const result = await api.simulate(sessionId, getPolicy());
            setNote(result.summary);
          })}>Preview cleanup</button>
          {shared > 0 && <p>{shared} item(s) may be shared. The cleanup policy keeps them.</p>}
          <label className="check">
            <input type="checkbox" checked={confirmCleanup} disabled={busy} onChange={(e) => setConfirmCleanup(e.target.checked)} /> I want to move the files selected by the cleanup policy
          </label>
          <button type="button" disabled={busy || !confirmCleanup} onClick={() => {
            setConfirmCleanup(false);
            void run(async () => setNote((await api.quarantineSession(sessionId, getPolicy())).summary));
          }}>Quarantine recorded files</button>
          <button type="button" disabled={busy} onClick={() => void run(async () => {
            const path = await api.exportReport(sessionId, getPolicy());
            setNote(`Saved report to ${path}.`);
          })}>Save report</button>
          <button type="button" disabled={busy} onClick={() => void run(async () => {
            setNote((await api.makePortable(sessionId)).summary);
          })}>Make portable copy</button>
        </>
      )}
      {error && <p role="alert" className="error">{error}</p>}
      {note && <p role="status" className="action-note">{note}</p>}
    </div>
  );
}
