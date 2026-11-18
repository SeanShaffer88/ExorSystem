import { useCallback, useEffect, useState } from "react";
import { flags } from "./flags";
import { Icon, Logo, type IconName } from "./components/Icon";
import { Badge } from "./components/Workspace";
import { OverviewView } from "./views/OverviewView";
import { ScanView } from "./views/ScanView";
import { ProvenanceView } from "./views/ProvenanceView";
import { SessionsView } from "./views/SessionsView";
import { AppearanceView } from "./views/AppearanceView";
import { ThemeUndoBar } from "./ThemeUndoBar";
import { WatchView } from "./views/WatchView";
import { QuarantineView } from "./views/QuarantineView";
import { SettingsView } from "./views/SettingsView";
import { RiskView } from "./views/RiskView";
import { HistoryView } from "./views/HistoryView";
import { SimulatorView } from "./views/SimulatorView";
import { TerminalView } from "./views/TerminalView";
import { StatusBar } from "./StatusBar";
import type { HistoryRow } from "./types";

type ViewId = "overview" | "sessions" | "watch" | "history" | "quarantine" | "simulator" | "provenance" | "terminal" | "scan" | "risk" | "settings" | "appearance";
type NavView = { id: ViewId; label: string; icon: IconName; enabled?: boolean };
const groups: { label: string; views: NavView[] }[] = [
  { label: "Workspace", views: [
    { id: "overview", label: "Overview", icon: "overview" },
    { id: "sessions", label: "Applications", icon: "apps" },
    { id: "watch", label: "Registry monitor", icon: "registry", enabled: flags.watchView },
    { id: "history", label: "Change inspector", icon: "changes" },
  ] },
  { label: "Cleanup", views: [
    { id: "quarantine", label: "Cleanup summary", icon: "shield", enabled: flags.quarantineView },
    { id: "simulator", label: "Policy simulator", icon: "sliders" },
    { id: "scan", label: "Scan folder", icon: "folder" },
  ] },
  { label: "Analysis", views: [
    { id: "provenance", label: "Relationships", icon: "graph" },
    { id: "terminal", label: "Script preview", icon: "terminal" },
    { id: "risk", label: "Update risk", icon: "pulse" },
  ] },
  { label: "Preferences", views: [
    { id: "settings", label: "Settings", icon: "settings" },
    { id: "appearance", label: "Appearance", icon: "palette", enabled: flags.themes },
  ] },
];
const enabled = groups.flatMap((g) => g.views).filter((v) => v.enabled !== false);

export function App({ demo = false }: { demo?: boolean }) {
  const [view, setView] = useState<ViewId>("overview");
  const [programId, setProgramId] = useState<string | null>(null);
  const [history, setHistory] = useState<{ sessionId: number | null; row?: HistoryRow }>({ sessionId: null });
  const [tracePath, setTracePath] = useState("");
  const [simulationSession, setSimulationSession] = useState<number | null>(null);
  const [terminalProgram, setTerminalProgram] = useState<string | null>(null);
  const [watching, setWatching] = useState(false);
  const current = enabled.find((v) => v.id === view)!;

  const go = useCallback((id: ViewId, focus = true) => {
    if (!enabled.some((v) => v.id === id)) return;
    setView(id);
    document.getElementById("main")?.scrollTo({ top: 0 });
    if (focus) document.getElementById(`tab-${id}`)?.focus();
  }, []);

  const openHistory = (sessionId: number | null, row?: HistoryRow) => {
    setHistory({ sessionId, row });
    go("history");
  };
  const openTrace = (path: string) => { setTracePath(path); go("provenance"); };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.ctrlKey || e.metaKey || document.querySelector("dialog[open]")) return;
      const target = enabled[Number(e.key) - 1];
      if (target) { e.preventDefault(); go(target.id); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const onTabKey = (e: React.KeyboardEvent) => {
    const i = enabled.findIndex((v) => v.id === view);
    const n = enabled.length;
    const next = e.key === "ArrowDown" || e.key === "ArrowRight" ? (i + 1) % n : e.key === "ArrowUp" || e.key === "ArrowLeft" ? (i + n - 1) % n : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : -1;
    const target = enabled[next];
    if (target) { e.preventDefault(); go(target.id); }
  };

  return (
    <div className="shell">
      <a className="skip" href="#main">Skip to content</a>
      <aside className="sidebar" aria-label="Workspace navigation">
        <div className="brand"><span className="brand-mark"><Logo/></span><div className="brand-copy"><strong>ExorSystem</strong><small>SYSTEM WORKSPACE</small></div></div>
        <div className="side-nav" role="tablist" aria-label="Tools" aria-orientation="vertical" onKeyDown={onTabKey}>
          {groups.map((group) => <div className="nav-group" key={group.label} role="presentation">
            <div className="nav-label" aria-hidden="true">{group.label}</div>
            {group.views.filter((v) => v.enabled !== false).map((v) => <button key={v.id} id={`tab-${v.id}`} type="button" role="tab" aria-selected={view === v.id} aria-controls="main"
              className="nav-tab" tabIndex={view === v.id ? 0 : -1} title={v.label} aria-description={v.id === "watch" && watching ? "Watch in progress" : undefined} onClick={() => go(v.id, false)}>
              <Icon name={v.icon} size={17}/><span className="nav-text">{v.label}</span>{v.id === "watch" && watching && <span className="nav-indicator" aria-hidden="true"/>}
            </button>)}
          </div>)}
        </div>
        <div className="side-profile"><Tile/><span className="profile-text"><strong>Local profile</strong><small>Driverless desktop</small></span></div>
      </aside>
      <header className="workspace-bar">
        <div className="breadcrumb"><Icon name="monitor" size={16}/><span>Workspace</span><Icon name="chevron" size={13}/><strong>{current.label}</strong></div>
        <div className="workspace-status">{demo && <Badge tone="review">Demo data</Badge>}<span className="local-status"><i className="dot tone-safe" aria-hidden="true"/> Local only</span></div>
      </header>
      <main id="main" role="tabpanel" aria-labelledby={`tab-${view}`} tabIndex={-1}>
        <ThemeUndoBar />
        {view === "overview" && <OverviewView onApplications={(id) => { setProgramId(id ?? null); go("sessions"); }} onWatch={() => go("watch")} onHistory={(row) => openHistory(row?.session_id ?? null, row)} onCleanup={() => go("quarantine")}/>}
        {view === "sessions" && <SessionsView initialProgramId={programId} onTrace={openTrace} onHistory={(id) => openHistory(id)} onSimulate={(id) => { setSimulationSession(id); go("simulator"); }} onCleanup={() => go("quarantine")} onTerminal={(id) => { setTerminalProgram(id); go("terminal"); }}/>}
        {/* Preserve the baseline and progress when visiting another dashboard view. */}
        {flags.watchView && <WatchView hidden={view !== "watch"} onOpenHistory={openHistory} onWatching={setWatching}/>}
        {view === "scan" && <ScanView />}
        {view === "provenance" && <ProvenanceView initialPath={tracePath}/>}
        {view === "simulator" && <SimulatorView initialSessionId={simulationSession}/>}
        {view === "quarantine" && <QuarantineView onSimulate={() => go("simulator")}/>}
        {view === "risk" && <RiskView />}
        {view === "history" && <HistoryView initialSessionId={history.sessionId} initialRow={history.row} onTrace={openTrace} onWatch={() => go("watch")}/>}
        {view === "terminal" && <TerminalView initialProgramId={terminalProgram} onInventory={() => go("sessions")}/>}
        {view === "settings" && <SettingsView />}
        {view === "appearance" && <AppearanceView />}
      </main>
      <StatusBar onTerminal={() => go("terminal")}/>
    </div>
  );
}

function Tile() {
  return <span className="profile-icon"><Icon name="lock" size={16}/></span>;
}
