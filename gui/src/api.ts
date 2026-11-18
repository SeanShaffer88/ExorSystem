import { Channel, invoke } from "@tauri-apps/api/core";
import type { AppInfo, TemplateInfo, HistoryPage, RiskView, PortableView, PortableProgram, LeftoverReview, UninstallRun, ProgramInventory, ProvenanceView, SessionItems, QuarantineResult, QuarantineRow, ScanEvent, Session, SimulationView, WatchStart, WatchSummary } from "./types";

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
  listPrograms: (includeSystem: boolean) => invoke<ProgramInventory>("list_programs", { includeSystem }),
  listPortablePrograms: () => invoke<PortableProgram[]>("list_portable_programs"),
  registerPortableProgram: (name: string, executable: string) => invoke<void>("register_portable_program", { name, executable }),
  forgetPortableProgram: (executable: string) => invoke<void>("forget_portable_program", { executable }),
  runUninstaller: (programId: string, quiet: boolean) => invoke<UninstallRun>("run_uninstaller", { programId, quiet }),
  findLeftovers: (programId: string) => invoke<LeftoverReview>("find_leftovers", { programId }),
  quarantineLeftovers: (programId: string, folders: string[], sessionId: number | null) =>
    invoke<QuarantineResult>("quarantine_leftovers", { programId, folders, sessionId }),
  history: (filter: string | null, limit: number, sessionId: number | null, beforeId: number | null) =>
    invoke<HistoryPage>("history", { filter, limit, sessionId, beforeId }),
  exportReport: (sessionId: number, template: string) => invoke<string>("export_report", { sessionId, template }),
  makePortable: (sessionId: number) => invoke<PortableView>("make_portable", { sessionId }),
  updateRisk: () => invoke<RiskView>("update_risk"),
  listTemplates: () => invoke<TemplateInfo[]>("list_templates"),
  appInfo: () => invoke<AppInfo>("app_info"),
  sessionItems: (sessionId: number) => invoke<SessionItems>("session_items", { sessionId }),
  beginWatch: (appName: string, installerPath: string | null, folders: string[], registrySubtrees: string) =>
    invoke<WatchStart>("begin_watch", { appName, installerPath, folders, registrySubtrees }),
  finishWatch: () => invoke<WatchSummary>("finish_watch"),
  cancelWatch: () => invoke<void>("cancel_watch"),
  cancelScan: () => invoke<void>("cancel_scan"),
};
