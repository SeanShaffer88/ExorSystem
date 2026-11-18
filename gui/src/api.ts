import { Channel } from "@tauri-apps/api/core";
import { invoke, isDemo } from "./bridge";
import type { AppInfo, BatchPreview, ChangeDomain, ChangeExport, ChangeKind, TemplateInfo, HistoryPage, RiskView, PortableView, PortableProgram, LeftoverReview, UninstallPreview, UninstallRun, ProgramInventory, ProvenanceView, SessionItems, QuarantineResult, QuarantineRow, ScanEvent, Session, SimulationView, WatchRun, WatchStart, WatchSummary, WatchSystem } from "./types";

function capture<T>(command: string, args: Record<string, unknown>, ready: () => void) {
  if (isDemo()) return invoke<T>(command, { ...args, onReady: ready });
  const onReady = new Channel<null>();
  onReady.onmessage = ready;
  return invoke<T>(command, { ...args, onReady });
}

export const api = {
  listSessions: () => invoke<Session[]>("list_sessions"),
  provenance: (path: string) => invoke<ProvenanceView>("provenance", { path }),
  simulate: (sessionId: number, template: string) => invoke<SimulationView>("simulate", { sessionId, template }),
  /** Streams progress through a Tauri v2 channel; resolves when the scan has ended. */
  scan: (root: string, onEvent: (e: ScanEvent) => void) => {
    if (isDemo()) return invoke<ScanEvent>("scan", { root }).then(onEvent);
    const channel = new Channel<ScanEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("scan", { root, onEvent: channel });
  },
  quarantineSession: (sessionId: number, template: string) => invoke<QuarantineResult>("quarantine_session", { sessionId, template }),
  listQuarantines: () => invoke<QuarantineRow[]>("list_quarantines"),
  quarantineBatch: (quarantineId: string, offset: number) => invoke<BatchPreview>("quarantine_batch", { quarantineId, offset }),
  restoreQuarantine: (quarantineId: string) => invoke<number>("restore_quarantine", { quarantineId }),
  purgeQuarantine: (quarantineId: string) => invoke<number>("purge_quarantine", { quarantineId }),
  listPrograms: (includeSystem: boolean) => invoke<ProgramInventory>("list_programs", { includeSystem }),
  listPortablePrograms: () => invoke<PortableProgram[]>("list_portable_programs"),
  registerPortableProgram: (name: string, executable: string) => invoke<void>("register_portable_program", { name, executable }),
  forgetPortableProgram: (executable: string) => invoke<void>("forget_portable_program", { executable }),
  previewUninstaller: (programId: string) => invoke<UninstallPreview>("preview_uninstaller", { programId }),
  runUninstaller: (programId: string, expected: UninstallPreview) => invoke<UninstallRun>("run_uninstaller", { programId, quiet: false, expected }),
  findLeftovers: (programId: string) => invoke<LeftoverReview>("find_leftovers", { programId }),
  quarantineLeftovers: (programId: string, folders: string[], sessionId: number | null) =>
    invoke<QuarantineResult>("quarantine_leftovers", { programId, folders, sessionId }),
  history: (filter: string | null, limit: number, sessionId: number | null, beforeId: number | null, domain: ChangeDomain | null = null, changeKind: ChangeKind | null = null) =>
    invoke<HistoryPage>("history", { filter, limit, sessionId, beforeId, domain, changeKind }),
  saveChanges: (filter: string | null, sessionId: number | null, changeKind: ChangeKind | null) =>
    invoke<ChangeExport>("save_changes", { filter, sessionId, changeKind }),
  watchComparison: (sessionId: number) => invoke<WatchSummary | null>("watch_comparison", { sessionId }),
  recentWatchRuns: () => invoke<WatchRun[]>("recent_watch_runs"),
  exportReport: (sessionId: number, template: string) => invoke<string>("export_report", { sessionId, template }),
  makePortable: (sessionId: number) => invoke<PortableView>("make_portable", { sessionId }),
  updateRisk: () => invoke<RiskView>("update_risk"),
  listTemplates: () => invoke<TemplateInfo[]>("list_templates"),
  appInfo: () => invoke<AppInfo>("app_info"),
  sessionItems: (sessionId: number, filter: string | null, offset: number) => invoke<SessionItems>("session_items", { sessionId, filter, offset }),
  beginWatch: (request: { appName: string; installerPath: string | null; folders: string[]; registrySubtrees: string; system: WatchSystem }, captureId: number, ready: () => void) =>
    capture<WatchStart | null>("begin_watch", { request, captureId }, ready),
  finishWatch: (captureId: number, ready: () => void) => capture<WatchSummary | null>("finish_watch", { captureId }, ready),
  stopWatchCapture: (captureId: number) => invoke<boolean>("stop_watch_capture", { captureId }),
  cancelWatch: () => invoke<void>("cancel_watch"),
  cancelScan: () => invoke<void>("cancel_scan"),
};
