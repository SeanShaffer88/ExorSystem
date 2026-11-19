import { Channel } from "@tauri-apps/api/core";
import { invoke, isDemo } from "./bridge";
import type { AppInfo, HistoryDates, LeftoverFolder, PlanRequest, PlanHeader, PlanPage, QuarantinePage, Usage, VerifiedCopy, WatchEvent, WatchProgress, BatchPreview, ChangeDomain, ChangeExport, ChangeKind, TemplateInfo, HistoryPage, RiskView, PortableView, PortableProgram, LeftoverReview, UninstallPreview, UninstallRun, ProgramInventory, ProvenanceView, SessionItems, QuarantineResult, ScanEvent, Session, SimulationView, WatchRun, WatchStart, WatchSummary, WatchSystem } from "./types";

function capture<T>(command: string, args: Record<string, unknown>, ready: () => void, progress?: (p: WatchProgress) => void) {
  const receive = (event: WatchEvent) => { if (event.event === "ready") ready(); else progress?.(event.data); };
  if (isDemo()) return invoke<T>(command, { ...args, onReady: receive });
  const onReady = new Channel<WatchEvent>();
  onReady.onmessage = receive;
  return invoke<T>(command, { ...args, onReady });
}

let sessions: { deadline: number; request: Promise<Session[]> } | null = null;
export function invalidateSessions() { sessions = null; }

function listSessions() {
  if (sessions && sessions.deadline > Date.now()) return sessions.request;
  const request = invoke<Session[]>("list_sessions").catch((e: unknown) => {
    if (sessions?.request === request) sessions = null;
    throw e;
  });
  sessions = { request, deadline: Date.now() + 10_000 };
  return request;
}

export const api = {
  listSessions,
  pickPath: (directory: boolean) => invoke<string | null>("pick_path", { directory }),
  provenance: (path: string) => invoke<ProvenanceView>("provenance", { path }),
  simulate: (sessionId: number, template: string, offset = 0) => invoke<SimulationView>("simulate", { sessionId, template, offset }),
  /** Streams progress through a Tauri v2 channel; resolves when the scan has ended. */
  scan: (root: string, strict: boolean, onEvent: (e: ScanEvent) => void) => {
    if (isDemo()) return invoke<ScanEvent>("scan", { root, strict }).then(onEvent);
    const channel = new Channel<ScanEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("scan", { root, strict, onEvent: channel });
  },
  cleanupFolders: (programId: string) => invoke<LeftoverFolder[]>("cleanup_folders", { programId }),
  linkCleanupSession: (sessionId: number, programId: string) => invoke<void>("link_cleanup_session", { sessionId, programId }),
  prepareCleanup: (request: PlanRequest) => invoke<PlanHeader>("prepare_cleanup", { request }),
  cleanupPlanPage: (id: number, offset: number, excluded: boolean) => invoke<PlanPage>("cleanup_plan_page", { id, offset, excluded }),
  applyCleanup: (id: number, approvedFiles: number) => invoke<QuarantineResult>("apply_cleanup", { id, approvedFiles }),
  discardCleanup: (id: number) => invoke<void>("discard_cleanup_plan", { id }),
  stopCleanup: () => invoke<boolean>("stop_cleanup_preflight"),
  quarantinePage: (offset = 0) => invoke<QuarantinePage>("list_quarantines", { offset }),
  listQuarantines: async () => (await invoke<QuarantinePage>("list_quarantines", { offset: 0 })).rows,
  storageUsage: () => invoke<Usage>("storage_usage"),
  backupProfile: (destination: string) => invoke<VerifiedCopy>("backup_profile", { destination }),
  restoreProfileCopy: (backupPath: string, destination: string) => invoke<VerifiedCopy>("restore_profile_copy", { backupPath, destination }),
  rotateArchivedBatch: (quarantineId: string, backupPath: string) => invoke<number>("rotate_archived_batch", { quarantineId, backupPath }),
  quarantineBatch: (quarantineId: string, offset: number) => invoke<BatchPreview>("quarantine_batch", { quarantineId, offset }),
  restoreQuarantine: (quarantineId: string) => invoke<number>("restore_quarantine", { quarantineId }),
  purgeQuarantine: (quarantineId: string) => invoke<number>("purge_quarantine", { quarantineId }),
  listPrograms: (includeSystem: boolean) => invoke<ProgramInventory>("list_programs", { includeSystem }),
  listPortablePrograms: () => invoke<PortableProgram[]>("list_portable_programs"),
  registerPortableProgram: (name: string, executable: string) => invoke<void>("register_portable_program", { name, executable }),
  forgetPortableProgram: (executable: string) => invoke<void>("forget_portable_program", { executable }),
  previewUninstaller: (programId: string) => invoke<UninstallPreview>("preview_uninstaller", { programId }),
  runUninstaller: (programId: string, expected: UninstallPreview, confirmUnverified = false) => invoke<UninstallRun>("run_uninstaller", { programId, quiet: false, expected, confirmUnverified }),
  findLeftovers: (programId: string) => invoke<LeftoverReview>("find_leftovers", { programId }),
  history: (filter: string | null, limit: number, sessionId: number | null, beforeId: number | null, domain: ChangeDomain | null = null, changeKind: ChangeKind | null = null, dates?: HistoryDates) =>
    invoke<HistoryPage>("history", { limit, filters: { filter, sessionId, beforeId, domain, changeKind, ...dates } }),
  saveChanges: (filter: string | null, sessionId: number | null, changeKind: ChangeKind | null, dates?: HistoryDates, beforeId: number | null = null) =>
    invoke<ChangeExport>("save_changes", { filters: { filter, sessionId, changeKind, beforeId, ...dates } }),
  watchComparison: (sessionId: number) => invoke<WatchSummary | null>("watch_comparison", { sessionId }),
  recentWatchRuns: () => invoke<WatchRun[]>("recent_watch_runs"),
  exportReport: (sessionId: number, template: string) => invoke<string>("export_report", { sessionId, template }),
  makePortable: (sessionId: number) => invoke<PortableView>("make_portable", { sessionId }),
  updateRisk: () => invoke<RiskView>("update_risk"),
  listTemplates: () => invoke<TemplateInfo[]>("list_templates"),
  appInfo: () => invoke<AppInfo>("app_info"),
  sessionItems: (sessionId: number, filter: string | null, offset: number) => invoke<SessionItems>("session_items", { sessionId, filter, offset }),
  beginWatch: (request: { appName: string; installerPath: string | null; folders: string[]; registrySubtrees: string; system: WatchSystem; strict?: boolean }, captureId: number, ready: () => void, progress?: (p: WatchProgress) => void) =>
    capture<WatchStart | null>("begin_watch", { request, captureId }, ready, progress).finally(invalidateSessions),
  finishWatch: (captureId: number, ready: () => void, progress?: (p: WatchProgress) => void) => capture<WatchSummary | null>("finish_watch", { captureId }, ready, progress).finally(invalidateSessions),
  stopWatchCapture: (captureId: number) => invoke<boolean>("stop_watch_capture", { captureId }),
  cancelWatch: () => invoke<void>("cancel_watch").finally(invalidateSessions),
  cancelScan: () => invoke<void>("cancel_scan"),
};
