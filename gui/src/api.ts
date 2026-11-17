import { invoke } from "@tauri-apps/api/core";
import type { ProvenanceView, Session, SimulationView } from "./types";

export const api = {
  listSessions: () => invoke<Session[]>("list_sessions"),
  provenance: (path: string) => invoke<ProvenanceView>("provenance", { path }),
  simulate: (sessionId: number, template: string) => invoke<SimulationView>("simulate", { sessionId, template }),
};
