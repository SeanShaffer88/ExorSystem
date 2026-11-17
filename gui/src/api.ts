import { Channel, invoke } from "@tauri-apps/api/core";
import type { ProvenanceView, QuarantineResult, QuarantineRow, ScanEvent, Session, SimulationView } from "./types";

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
  cancelScan: () => invoke<void>("cancel_scan"),
};
