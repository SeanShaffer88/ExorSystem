import { useLayoutEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { Icon } from "../components/Icon";
import { Badge, CardHeading } from "../components/Workspace";
import type { InstalledProgram } from "../types";
import { hasVendorCommand } from "./TerminalView";
import { UninstallWizard, type VendorReviewOutcome } from "./UninstallWizard";

export const MAX_QUEUED_PROGRAMS = 12;

type Target = Pick<InstalledProgram, "id" | "name" | "publisher" | "version" | "scope" | "install_location" | "install_date">;
type Result = VendorReviewOutcome | { kind: "skipped" };
export type QueuedProgram = { target: Target; result: Result | null };

export function queueEligible(program: InstalledProgram, removed: boolean) {
  return !removed && !program.system_component && !program.parent_key && hasVendorCommand(program);
}

// A queue entry is a display identity only. Never retain an invocation or an approval here.
export function queueTarget({ id, name, publisher, version, scope, install_location, install_date }: InstalledProgram): Target {
  return { id, name, publisher, version, scope, install_location, install_date };
}

function sameTarget(saved: Target, live: InstalledProgram) {
  return saved.id === live.id && saved.name === live.name && saved.publisher === live.publisher &&
    saved.version === live.version && saved.scope === live.scope && saved.install_location === live.install_location &&
    saved.install_date === live.install_date;
}

function status(result: Result | null) {
  switch (result?.kind) {
    case "exited": return result.run.exit_code === null ? "Exit code unknown" : `Exit code ${result.run.exit_code}`;
    case "unconfirmed": return "Outcome unconfirmed";
    case "blocked": return "Preview blocked";
    case "skipped": return "Skipped";
    default: return "Waiting for review";
  }
}

export function UninstallQueue({ items, setItems, programs, inventoryReady, selecting, onVendorExit }: {
  items: QueuedProgram[];
  setItems: Dispatch<SetStateAction<QueuedProgram[]>>;
  programs: ReadonlyMap<string, InstalledProgram>;
  inventoryReady: boolean;
  selecting: boolean;
  onVendorExit: () => void;
}) {
  const [active, setActive] = useState<QueuedProgram | null>(null);
  const region = useRef<HTMLElement>(null);
  const trigger = useRef<HTMLButtonElement | null>(null);

  useLayoutEffect(() => {
    if (active || !trigger.current) return;
    const button = trigger.current;
    trigger.current = null;
    (button.isConnected && !button.disabled ? button : region.current)?.focus({ preventScroll: true });
  }, [active]);

  const availability = (item: QueuedProgram): string | null => {
    if (!inventoryReady) return "Refresh or wait for inventory before reviewing.";
    const live = programs.get(item.target.id);
    if (!live) return "No matching eligible registration in the current inventory.";
    if (!sameTarget(item.target, live)) return "Inventory details changed. Remove this entry and select the current program again.";
    return null;
  };
  const update = (id: string, result: Result) => setItems((list) => list.map((item) => item.target.id === id ? { ...item, result } : item));
  const review = (item: QueuedProgram, button: HTMLButtonElement) => {
    if (active || availability(item) || (item.result?.kind === "exited" && item.result.run.exit_code === 0)) return;
    trigger.current = button;
    setActive(item);
  };
  const next = items.find((item) => item.result === null);

  return <section className="card uninstall-queue" aria-label="Uninstall review queue" ref={region} tabIndex={-1}>
    <CardHeading title="Uninstall review queue" icon="shield" tone="review" subtitle="Confirm one live vendor invocation at a time. Leftover review is separate." action={<Badge>{items.length} / {MAX_QUEUED_PROGRAMS} queued</Badge>}/>
    <div className="queue-body">
      <p className="muted">{selecting ? "Use the inventory checkboxes to pick supported classic apps. " : ""}The queue stays in this view, is not saved, and never starts or retries an item by itself.</p>
      {items.length > 0 ? <>
        <ol className="queue-items" aria-label="Queued applications">
          {items.map((item) => {
            const { target, result } = item;
            const exited = result?.kind === "exited";
            const finished = exited && result.run.exit_code === 0;
            const unavailable = finished ? null : availability(item);
            return <li key={target.id} className="queue-item">
              <div className="queue-item-copy">
                <strong>{target.name}</strong>
                <small>{[target.publisher, target.version].filter(Boolean).join(" · ") || "Version not reported"}</small>
                <div role="status" className="queue-result">
                  <Badge tone={result && !finished && result.kind !== "skipped" ? "review" : "neutral"}>{active?.target.id === target.id ? "Review open" : status(result)}</Badge>
                  {exited && <small>Check Windows to confirm what was removed.</small>}
                  {result?.kind === "unconfirmed" && <small>{result.message} Check Windows before retrying.</small>}
                  {result?.kind === "blocked" && <small>{result.message}</small>}
                  {unavailable && <small>{unavailable}</small>}
                </div>
              </div>
              <div className="button-row">
                {result === null && <button className="quiet-button" type="button" aria-label={`Skip ${target.name}`} onClick={() => update(target.id, { kind: "skipped" })}>Skip</button>}
                {result && !finished && <button className="quiet-button" type="button" aria-label={`Review ${target.name} again`} disabled={!!active || !!unavailable} onClick={(e) => review(item, e.currentTarget)}>Review again</button>}
                <button className="text-button" type="button" aria-label={`Remove ${target.name} from queue`} onClick={() => setItems((list) => list.filter((row) => row.target.id !== target.id))}>Remove from queue</button>
              </div>
            </li>;
          })}
        </ol>
        <div className="button-row">
          <button className="primary" type="button" disabled={!!active || !next || !!availability(next)} onClick={(e) => { if (next) review(next, e.currentTarget); }}><Icon name="search"/> Review next uninstaller</button>
          <button className="quiet-button" type="button" onClick={() => setItems([])}>Clear queue</button>
        </div>
      </> : <p className="muted" role="status">No applications queued. Adding an application does not run anything.</p>}
    </div>
    {active && <UninstallWizard key={active.target.id} program={active.target} mode="queue" retry={active.result?.kind === "exited" || active.result?.kind === "unconfirmed"} onUninstallComplete={onVendorExit} onClose={(result) => {
      if (result) update(active.target.id, result);
      setActive(null);
    }}/>}
  </section>;
}
