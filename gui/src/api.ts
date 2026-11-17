import { Channel, invoke } from "@tauri-apps/api/core";
import type { HistoryRow, RiskView, PortableView, Leftover, UninstallRun, ProgramRow, ProvenanceView, SessionItems, QuarantineResult, QuarantineRow, ScanEvent, Session, SimulationView, WatchSummary } from "./types";

export const api = {
  listSessions: () => invoke<Session[]>("list_sessions"),
  provenance: (path: string) => invoke<ProvenanceView>("provenance", { path }),
  simulate: (sessionId: number, template: string) => invoke<SimulationView>("simulate", { sessionId, template }),
  /** Streams progress through a Tauri v2 channel; resolves when the scan has ended. */
  scan: (root: string, onEvent: (e: ScanEvent) => void) => {
    const channel = new Channel<ScanEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("scan", { root, onEvent: channel });
  },
  quarantineSession: (sessionId: number, template: string) => invoke<QuarantineResult>("quarantine_session", { sessionId, template }),
  listQuarantines: () => invoke<QuarantineRow[]>("list_quarantines"),
  restoreQuarantine: (quarantineId: string) => invoke<number>("restore_quarantine", { quarantineId }),
  purgeQuarantine: (quarantineId: string) => invoke<number>("purge_quarantine", { quarantineId }),
  listPrograms: (includeSystem: boolean) => invoke<ProgramRow[]>("list_programs", { includeSystem }),
  runUninstaller: (programId: string, quiet: boolean) => invoke<UninstallRun>("run_uninstaller", { programId, quiet }),
  findLeftovers: (programId: string) => invoke<Leftover[]>("find_leftovers", { programId }),
  quarantineLeftovers: (programId: string, folders: string[], sessionId: number | null) =>
    invoke<QuarantineResult>("quarantine_leftovers", { programId, folders, sessionId }),
  history: (filter: string | null, limit: number) => invoke<HistoryRow[]>("history", { filter, limit }),
  exportReport: (sessionId: number, template: string) => invoke<string>("export_report", { sessionId, template }),
  makePortable: (sessionId: number) => invoke<PortableView>("make_portable", { sessionId }),
  updateRisk: () => invoke<RiskView>("update_risk"),
  sessionItems: (sessionId: number) => invoke<SessionItems>("session_items", { sessionId }),
  beginWatch: (appName: string, installerPath: string | null, folders: string[]) =>
    invoke<number>("begin_watch", { appName, installerPath, folders }),
  finishWatch: () => invoke<WatchSummary>("finish_watch"),
  cancelWatch: () => invoke<void>("cancel_watch"),
  cancelScan: () => invoke<void>("cancel_scan"),
};
