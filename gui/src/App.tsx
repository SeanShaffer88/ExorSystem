import { useCallback, useEffect, useLayoutEffect, useReducer, useState } from "react";
import { useActivityLog } from "./components/ActivityLog";
import { Icon, Logo } from "./components/Icon";
import { Badge } from "./components/Workspace";
import { Disclosure } from "./docking/Disclosure";
import { DockWorkspace, tabId } from "./docking/DockWorkspace";
import { dockName, dockReducer, locationOf, readLayout, saveLayout, workspaceState, type DockArea, type LayoutPreset } from "./docking/layout";
import { PanelModule, type Inputs, type Requests } from "./docking/PanelModule";
import { panel, quickTools, toolGroups, tools, type PanelId } from "./docking/panels";
import { StatusBar } from "./StatusBar";
import { ThemeUndoBar } from "./ThemeUndoBar";

const presets = [
  { id: "default", title: "Reset layout", detail: "Overview and inventory in one dock" },
  { id: "investigate", title: "Investigation layout", detail: "Evidence, Watch and activity together" },
  { id: "cleanup", title: "Cleanup layout", detail: "Batches beside a dry run; no actions run" },
] as const;

export function App({ demo = false }: { demo?: boolean }) {
  const [state, dispatch] = useReducer(dockReducer, null, () => workspaceState(readLayout()));
  const { layout } = state;
  const current = layout.groups[layout.focus].active;
  const [compact, setCompact] = useState(false);
  const [requests, setRequests] = useState<Requests>({});
  const [watching, setWatching] = useState(false);
  const [focusRequest, setFocusRequest] = useState<{ id?: PanelId } | null>(null);
  const log = useActivityLog();

  useEffect(() => saveLayout(layout), [layout]);
  useLayoutEffect(() => {
    if (!focusRequest) return;
    const id = focusRequest.id ?? current;
    const target = id ? document.getElementById(tabId(id)) : null;
    if (target?.getClientRects().length) target.focus();
    else document.getElementById("main")?.focus();
    setFocusRequest(null);
  }, [current, focusRequest]);

  const open = useCallback((id: PanelId, focus = true) => {
    dispatch({ kind: "open", id });
    if (focus) setFocusRequest({ id });
  }, []);
  const close = useCallback((id: PanelId) => {
    dispatch({ kind: "close", id }); setFocusRequest({});
  }, []);
  const move = (id: PanelId, area: DockArea, before?: PanelId) => {
    dispatch({ kind: "move", id, area, before }); setFocusRequest({ id });
  };
  const apply = (preset: LayoutPreset) => {
    dispatch({ kind: "preset", preset }); setFocusRequest({});
  };
  const request = <K extends keyof Inputs,>(id: K, value: Inputs[K]) => {
    setRequests((old) => ({ ...old, [id]: { value, revision: (old[id]?.revision ?? 0) + 1 } }));
    open(id);
  };

  const logArea = locationOf(layout, "activity");
  const logVisible = !!logArea && layout.groups[logArea].active === "activity" && (!compact || layout.focus === logArea);
  const toggleLog = useCallback(() => { if (logVisible) close("activity"); else open("activity"); }, [close, logVisible, open]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (document.querySelector("dialog[open]")) return;
      if (e.ctrlKey && !e.altKey && e.key === "`") {
        e.preventDefault(); if (!e.repeat) toggleLog(); return;
      }
      if (!e.altKey || e.ctrlKey || e.metaKey) return;
      const target = quickTools[Number(e.key) - 1];
      if (target) { e.preventDefault(); open(target); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, toggleLog]);

  const onToolKey = (e: React.KeyboardEvent) => {
    const target = (e.target as HTMLElement).closest<HTMLButtonElement>("button[data-tool]");
    const i = tools.indexOf(target?.dataset.tool as PanelId);
    if (i < 0) return;
    const n = tools.length;
    const next = e.key === "ArrowDown" || e.key === "ArrowRight" ? (i + 1) % n : e.key === "ArrowUp" || e.key === "ArrowLeft" ? (i + n - 1) % n : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : -1;
    const id = tools[next];
    if (id) { e.preventDefault(); open(id, false); document.getElementById(`tool-${id}`)?.focus(); }
  };

  // Tauri's file drop handler is off for the main WebView to let HTML tab drags through.
  // Dock targets handle internal drags; do not let other drops navigate the WebView or insert data.
  return <div className="shell" onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "none"; }} onDrop={(e) => e.preventDefault()}>
    <a className="skip" href="#main" onClick={(e) => {
      e.preventDefault(); (current && document.getElementById(`panel-${current}`) || document.getElementById("main"))?.focus();
    }}>Skip to workspace</a>
    <aside className="sidebar" aria-label="Workspace navigation">
      <div className="brand"><span className="brand-mark"><Logo/></span><div className="brand-copy"><strong>ExorSystem</strong><small>SYSTEM WORKSPACE</small></div></div>
      <div className="side-nav" role="toolbar" aria-orientation="vertical" aria-label="Workspace tools" onKeyDown={onToolKey}>
        {toolGroups.map((group) => <div className="nav-group" key={group.label}>
          <div className="nav-label" aria-hidden="true">{group.label}</div>
          {group.ids.map((id) => {
            const spec = panel(id);
            const area = locationOf(layout, id);
            const index = quickTools.indexOf(id);
            const description = [area && `Open in ${dockName[area]}`, id === "watch" && watching && "Watch in progress"].filter(Boolean).join(". ");
            return <button key={id} id={`tool-${id}`} data-tool={id} data-open={!!area} type="button" className="nav-tab"
              aria-current={current === id ? "page" : undefined} aria-keyshortcuts={index >= 0 ? `Alt+${index + 1}` : undefined}
              tabIndex={(current ?? tools[0]) === id ? 0 : -1} title={spec.title} aria-description={description || undefined} onClick={() => open(id, false)}>
              <Icon name={spec.icon} size={17}/><span className="nav-text">{spec.title}</span>
              {id === "watch" && watching && <span className="nav-indicator" aria-hidden="true"/>}
            </button>;
          })}
        </div>)}
      </div>
      <div className="side-profile"><span className="profile-icon"><Icon name="lock" size={16}/></span><span className="profile-text"><strong>Local profile</strong><small>Driverless desktop</small></span></div>
    </aside>
    <header className="workspace-bar">
      <div className="breadcrumb"><Icon name="monitor" size={16}/><span>Workspace</span><Icon name="chevron" size={13}/><strong>{current ? panel(current).title : "Main dock"}</strong></div>
      <div className="workspace-status">
        {demo && <Badge tone="review">Demo data</Badge>}
        <span className="local-status"><i className="dot tone-safe" aria-hidden="true"/> Local only</span>
        <Disclosure label="Workspace layout" icon="layout" text="Layout">{(hide) => <>
          <strong>Workspace layout</strong>
          {presets.map(({ id, title, detail }) => <button key={id} type="button" className="layout-preset" onClick={() => { hide(); apply(id); }}>
            <span className="preset-diagram" data-preset={id} aria-hidden="true"><i/><i/><i/></span>
            <span>{title}<small>{detail}</small></span>
          </button>)}
          <small className="muted">Drag a tab or use its dock button. Layout preferences are saved locally. Reset does not erase evidence or cancel a watch.</small>
        </>}</Disclosure>
      </div>
    </header>
    <main id="main" className="workspace-main" aria-label="Dockable workspace" tabIndex={-1}>
      <ThemeUndoBar />
      <DockWorkspace state={state} compact={compact} onCompact={setCompact} onOpen={open} onMove={move} onClose={close}
        onFocus={(area) => dispatch({ kind: "focus", area })} onResize={(area, value) => dispatch({ kind: "resize", area, value })}
        renderPanel={(id, visible) => <PanelModule id={id} visible={visible} requests={requests} navigation={{ open, request }} log={log} onWatching={setWatching}/>}/>
    </main>
    <StatusBar last={log.lines.at(-1)?.msg ?? "Ready"} logOpen={logVisible} onLog={toggleLog} onTerminal={() => open("terminal")}/>
  </div>;
}
