import { Channel, invoke } from "@tauri-apps/api/core";
import type { ProvenanceView, ScanEvent, Session, SimulationView } from "./types";

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
  cancelScan: () => invoke<void>("cancel_scan"),
};
