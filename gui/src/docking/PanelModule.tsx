import type { ActivityLogState } from "../components/ActivityLog";
import type { HistoryRow } from "../types";
import { ActivityView } from "../views/ActivityView";
import { AppearanceView } from "../views/AppearanceView";
import { HistoryView } from "../views/HistoryView";
import { OverviewView } from "../views/OverviewView";
import { ProvenanceView } from "../views/ProvenanceView";
import { QuarantineView } from "../views/QuarantineView";
import { RiskView } from "../views/RiskView";
import { ScanView } from "../views/ScanView";
import { SessionsView } from "../views/SessionsView";
import { SettingsView } from "../views/SettingsView";
import { SimulatorView } from "../views/SimulatorView";
import { TerminalView } from "../views/TerminalView";
import { WatchView } from "../views/WatchView";
import type { PanelId } from "./panels";

export type Inputs = {
  sessions: string;
  history: { sessionId: number | null; row?: HistoryRow };
  provenance: string;
  simulator: number;
  terminal: string;
};
export type Requests = { [K in keyof Inputs]?: { value: Inputs[K]; revision: number } };
export type Navigation = {
  open: (id: PanelId) => void;
  request: <K extends keyof Inputs>(id: K, value: Inputs[K]) => void;
};

// The dock only knows panel IDs. The module owns domain views and the contextual links between them.
export function PanelModule({ id, visible, requests, navigation, log, onWatching }: {
  id: PanelId; visible: boolean; requests: Requests; navigation: Navigation; log: ActivityLogState; onWatching: (active: boolean) => void;
}) {
  const { open, request } = navigation;
  const inspect = (sessionId: number | null, row?: HistoryRow) => request("history", { sessionId, row });
  const trace = (path: string) => request("provenance", path);
  const watch = () => open("watch");
  const cleanup = () => open("quarantine");
  switch (id) {
    case "overview": return <OverviewView visible={visible} onApplications={(programId) => programId ? request("sessions", programId) : open("sessions")}
      onWatch={watch} onHistory={(row) => inspect(row?.session_id ?? null, row)} onCleanup={cleanup}/>;
    case "sessions": return <SessionsView key={requests.sessions?.revision} visible={visible} initialProgramId={requests.sessions?.value ?? null}
      onTrace={trace} onHistory={inspect} onSimulate={(sessionId) => request("simulator", sessionId)} onCleanup={cleanup} onTerminal={(programId) => request("terminal", programId)}/>;
    case "watch": return <WatchView hidden={!visible} onOpenHistory={inspect} onWatching={onWatching}/>;
    case "history": return <HistoryView key={requests.history?.revision} visible={visible} initialSessionId={requests.history?.value.sessionId ?? null}
      initialRow={requests.history?.value.row} onTrace={trace} onWatch={watch}/>;
    case "provenance": return <ProvenanceView key={requests.provenance?.revision} initialPath={requests.provenance?.value ?? ""}/>;
    case "simulator": return <SimulatorView key={requests.simulator?.revision} visible={visible} initialSessionId={requests.simulator?.value ?? null}/>;
    case "quarantine": return <QuarantineView visible={visible} onSimulate={() => open("simulator")}/>;
    case "terminal": return <TerminalView key={requests.terminal?.revision} visible={visible} initialProgramId={requests.terminal?.value ?? null} onInventory={() => open("sessions")}/>;
    case "activity": return <ActivityView visible={visible} log={log}/>;
    case "appearance": return <AppearanceView visible={visible}/>;
    case "settings": return <SettingsView/>;
    case "scan": return <ScanView/>;
    case "risk": return <RiskView visible={visible}/>;
  }
}
