import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import type { InstalledProgram, ProgramInventory, Session, SessionItems } from "../types";
import { ProgramActions } from "./ProgramActions";

/** One row in the left pane: an installed program, or a recorded install Windows does not list. */
interface Entry {
  key: string;
  name: string;
  sub: string;
  program: InstalledProgram | null;
  sessionIds: number[];
  removed: boolean;
}

const sourceText = { registry: "Classic Windows program", msix: "MSIX / Store app", portable: "Portable app" } as const;
const scopeText = { machine: "All users", machine32: "All users (32-bit)", user: "Current user" } as const;

function sizeText(kb: number | null) {
  if (kb === null) return "Unknown";
  return kb >= 1048576 ? `${(kb / 1048576).toFixed(1)} GB` : kb >= 1024 ? `${(kb / 1024).toFixed(1)} MB` : `${kb} KB`;
}

function sure(c: number) {
  return c >= 1 ? "Certain" : c >= 0.7 ? "Likely" : "Possible";
}

export function SessionsView() {
  const [inventory, setInventory] = useState<ProgramInventory | null>(null);
  const [revision, setRevision] = useState(0);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [items, setItems] = useState<SessionItems | null>(null);
  const [detailError, setDetailError] = useState("");
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    let live = true;
    setError("");
    Promise.all([api.listPrograms(showSystem), api.listSessions()])
      .then(([result, sessions]) => {
        if (!live) return;
        setInventory(result);
        setSessions(sessions);
      })
      .catch((e: unknown) => live && setError(String(e)));
    return () => { live = false; };
  }, [showSystem, revision]);

  const entries = useMemo<Entry[]>(() => {
    if (!inventory) return [];
    const rows = inventory.programs;
    const matched = new Set(rows.flatMap((r) => r.session_ids));
    const fromPrograms = rows.map((r) => ({
      key: `p:${r.program.id}`,
      name: r.program.name,
      sub: [r.removed ? "Recently removed" : sourceText[r.program.source], r.program.version, r.program.publisher].filter(Boolean).join(" · "),
      program: r.program,
      sessionIds: r.session_ids,
      removed: r.removed,
    }));
    const recordedOnly = sessions
      .filter((s) => !matched.has(s.session_id))
      .map((s) => ({ key: `s:${s.session_id}`, name: s.app_name, sub: "Recorded install", program: null, sessionIds: [s.session_id], removed: false }));
    return [...fromPrograms, ...recordedOnly].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  }, [inventory, sessions]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? entries.filter((e) => `${e.name} ${e.sub}`.toLowerCase().includes(q)) : entries;
  }, [entries, query]);

  const current = shown.find((e) => e.key === selected) ?? null;
  const sessionId = current?.sessionIds[0] ?? null;

  useEffect(() => {
    setItems(null);
    setDetailError("");
    if (sessionId === null) return;
    let live = true;
    api.sessionItems(sessionId).then((i) => live && setItems(i)).catch((e: unknown) => live && setDetailError(String(e)));
    return () => { live = false; };
  }, [sessionId, selected]);

  const move = (delta: number | "first" | "last") => {
    if (shown.length === 0) return;
    const i = shown.findIndex((e) => e.key === selected);
    const next = delta === "first" ? 0 : delta === "last" ? shown.length - 1 : Math.min(shown.length - 1, Math.max(0, (i < 0 ? 0 : i + delta)));
    const e = shown[next];
    if (e) {
      setSelected(e.key);
      document.getElementById(`opt-${e.key}`)?.scrollIntoView({ block: "nearest" });
    }
  };

  const onKey = (e: React.KeyboardEvent) => {
    const map: Record<string, number | "first" | "last"> = { ArrowDown: 1, ArrowUp: -1, Home: "first", End: "last", PageDown: 10, PageUp: -10 };
    const d = map[e.key];
    if (d !== undefined) {
      e.preventDefault();
      move(d);
    }
  };

  const p = current?.program ?? null;
  return (
    <section className="panes" aria-labelledby="h-inst">
      <div className="pane pane-list">
        <h2 id="h-inst">Installed programs</h2>
        <label htmlFor="prog-search" className="sr-only">Search programs</label>
        <input id="prog-search" type="search" placeholder="Search" value={query} onChange={(e) => setQuery(e.target.value)} title="Filter by name, source, version or publisher" />
        <label className="check">
          <input type="checkbox" checked={showSystem} onChange={(e) => setShowSystem(e.target.checked)} /> Show system components
        </label>
        {error && <p role="alert" className="error">Could not load programs: {error}</p>}
        {inventory?.warnings.map((w) => <p key={w} role="status">{w}</p>)}
        {inventory === null ? (
          !error && <p>Loading…</p>
        ) : shown.length === 0 ? (
          <p>{entries.length === 0 ? "Nothing to list. The program list comes from Windows; installs you record also appear here." : "No program matches your search."}</p>
        ) : (
          <>
            <p className="count" aria-live="polite">{shown.length} item{shown.length === 1 ? "" : "s"}</p>
            <ul ref={listRef} role="listbox" tabIndex={0} aria-label="Installed programs" aria-activedescendant={current ? `opt-${current.key}` : undefined} onKeyDown={onKey} onFocus={() => setSelected((previous) => shown.some((entry) => entry.key === previous) ? previous : shown[0]?.key ?? null)} className="listbox">
              {shown.map((e) => (
                <li key={e.key} id={`opt-${e.key}`} role="option" aria-selected={e.key === selected} onClick={() => { setSelected(e.key); listRef.current?.focus(); }}>
                  <span className="name">{e.name}</span>
                  <small>{e.sub || " "}</small>
                  {e.sessionIds.length > 0 && <span className="badge">Recorded</span>}
                </li>
              ))}
            </ul>
          </>
        )}
      </div>

      <div className="pane pane-detail" aria-live="polite">
        {!current ? (
          <p>Select a program to see its details and what its installation left behind.</p>
        ) : (
          <>
            <h2>{current.name}</h2>
            {p ? (
              <dl className="facts">
                <dt>Source</dt><dd>{sourceText[p.source]}</dd>
                <dt>Version</dt><dd>{p.version ?? "Unknown"}</dd>
                <dt>Publisher</dt><dd>{p.publisher ?? "Unknown"}</dd>
                <dt>Installed</dt><dd>{p.install_date ?? "Unknown"}</dd>
                <dt>Size</dt><dd>{sizeText(p.size_kb)}</dd>
                <dt>Location</dt><dd>{p.install_location ?? "Unknown"}</dd>
                <dt>For</dt><dd>{scopeText[p.scope]}</dd>
                {p.source === "registry" && <><dt>Uninstall command</dt><dd><code>{current.removed ? "Entry no longer listed by Windows" : p.uninstall_command ?? "None registered"}</code></dd></>}
              </dl>
            ) : (
              <p>This install was recorded by ExorSystem but is not in Windows' list of programs. It may be portable, or already uninstalled.</p>
            )}

            <h3>What this installation left behind</h3>
            {sessionId === null ? (
              <p>No recorded installation is linked to this entry.</p>
            ) : detailError ? (
              <p role="alert" className="error">{detailError}</p>
            ) : items === null ? (
              <p>Loading…</p>
            ) : (
              <>
                <p>
                  {items.total} item(s) traced
                  {Object.entries(items.by_type).length > 0 && ": " + Object.entries(items.by_type).map(([t, n]) => `${n} ${t}`).join(", ")}
                  {items.shared > 0 && `. ${items.shared} may be shared with other programs.`}
                </p>
                <div className="table-wrap">
                  <table>
                    <caption className="sr-only">Items traced to this installation</caption>
                    <thead><tr><th scope="col">Item</th><th scope="col">Type</th><th scope="col">Sure</th></tr></thead>
                    <tbody>
                      {items.items.map((i) => (
                        <tr key={i.path}><td>{i.path}{i.shared ? " (shared)" : ""}</td><td>{i.node_type}</td><td>{sure(i.confidence)}</td></tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {items.total > items.items.length && <p>Showing the first {items.items.length} of {items.total}.</p>}
              </>
            )}
          </>
        )}
      </div>
      {current ? (
        <ProgramActions key={`${current.key}:${sessionId}`} program={p} removed={current.removed} sessionId={sessionId} shared={items?.shared ?? 0}
          onUninstallComplete={() => setRevision((r) => r + 1)} />
      ) : (
        <div className="pane pane-actions"><h2>Actions</h2><p>Select a program.</p></div>
      )}
    </section>
  );
}
