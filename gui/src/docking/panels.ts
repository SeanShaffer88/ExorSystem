import type { IconName } from "../components/Icon";
import { flags } from "../flags";

type PanelSpec = { title: string; icon: IconName; enabled?: boolean };
const catalog = {
  overview: { title: "Overview", icon: "overview" },
  sessions: { title: "Applications", icon: "apps" },
  watch: { title: "Registry monitor", icon: "registry", enabled: flags.watchView },
  history: { title: "Change inspector", icon: "changes" },
  quarantine: { title: "Cleanup summary", icon: "shield", enabled: flags.quarantineView },
  simulator: { title: "Policy simulator", icon: "sliders" },
  scan: { title: "Scan folder", icon: "folder" },
  provenance: { title: "Relationships", icon: "graph" },
  risk: { title: "Update risk", icon: "pulse" },
  terminal: { title: "Script preview", icon: "terminal" },
  activity: { title: "Activity log", icon: "list" },
  settings: { title: "Settings", icon: "settings" },
  appearance: { title: "Appearance", icon: "palette", enabled: flags.themes },
} satisfies Record<string, PanelSpec>;

export type PanelId = keyof typeof catalog;
export const panel = (id: PanelId): PanelSpec => catalog[id];
export const isPanel = (id: unknown): id is PanelId =>
  typeof id === "string" && Object.hasOwn(catalog, id) && panel(id as PanelId).enabled !== false;

const groups: { label: string; ids: PanelId[] }[] = [
  { label: "Workspace", ids: ["overview", "sessions", "watch", "history"] },
  { label: "Cleanup", ids: ["quarantine", "simulator", "scan"] },
  { label: "Analysis", ids: ["provenance", "risk"] },
  { label: "Output", ids: ["terminal", "activity"] },
  { label: "Preferences", ids: ["settings", "appearance"] },
];
export const toolGroups = groups.map(({ label, ids }) => ({ label, ids: ids.filter(isPanel) }));
export const tools = toolGroups.flatMap(({ ids }) => ids);

// Keep the original Alt+1 through Alt+9 destinations independent of sidebar grouping.
export const quickTools: PanelId[] = (["overview", "sessions", "watch", "history", "quarantine", "simulator", "scan", "provenance", "terminal"] as const).filter(isPanel);
