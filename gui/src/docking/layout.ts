import { isPanel, type PanelId } from "./panels";

export const dockAreas = ["main", "right", "bottom"] as const;
export type DockArea = typeof dockAreas[number];
export type SplitArea = Exclude<DockArea, "main">;
export const dockName: Record<DockArea, string> = { main: "Main", right: "Right dock", bottom: "Bottom dock" };
export const splitSizes = {
  right: { min: 34, max: 56, start: 39 },
  bottom: { min: 26, max: 52, start: 32 },
} as const;

export type DockGroup = { ids: PanelId[]; active: PanelId | null };
export type Layout = {
  version: 1;
  groups: Record<DockArea, DockGroup>;
  focus: DockArea;
  sizes: Record<SplitArea, number>;
  reopen: Partial<Record<PanelId, DockArea>>;
};
export type LayoutPreset = "default" | "investigate" | "cleanup";
export const layoutKey = "exor.workspace.layout.v1";

function group(ids: PanelId[], active = ids[0] ?? null): DockGroup {
  return { ids, active };
}

export function preset(name: LayoutPreset): Layout {
  const config: Record<LayoutPreset, Record<DockArea, PanelId[]>> = {
    default: { main: ["overview", "sessions"], right: [], bottom: [] },
    investigate: { main: ["history", "sessions", "overview"], right: ["watch", "provenance"], bottom: ["activity", "terminal"] },
    cleanup: { main: ["quarantine", "sessions", "overview"], right: ["simulator"], bottom: ["activity", "terminal"] },
  };
  return {
    version: 1,
    groups: {
      main: group(config[name].main.filter(isPanel)),
      right: group(config[name].right.filter(isPanel)),
      bottom: group(config[name].bottom.filter(isPanel)),
    },
    focus: "main",
    sizes: { right: splitSizes.right.start, bottom: splitSizes.bottom.start },
    reopen: {},
  };
}

function object(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}

export function clampSize(area: SplitArea, value: unknown) {
  const { min, max, start } = splitSizes[area];
  return typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, Math.round(value))) : start;
}

/** Treat the saved layout as preferences, not as trusted application state. */
export function restoreLayout(value: unknown): Layout {
  const root = object(value);
  const raw = object(root?.groups);
  if (root?.version !== 1 || !raw) return preset("default");
  const seen = new Set<PanelId>();
  const read = (area: DockArea): DockGroup => {
    const entry = object(raw[area]);
    const ids: PanelId[] = [];
    if (Array.isArray(entry?.ids)) {
      for (const id of entry.ids) {
        if (!isPanel(id) || seen.has(id)) continue;
        seen.add(id); ids.push(id);
      }
    }
    const active = isPanel(entry?.active) && ids.includes(entry.active) ? entry.active : ids[0] ?? null;
    return group(ids, active);
  };
  const groups = { main: read("main"), right: read("right"), bottom: read("bottom") };
  const intentionallyEmpty = dockAreas.every((area) => {
    const entry = object(raw[area]);
    return Array.isArray(entry?.ids) && entry.ids.length === 0;
  });
  if (!seen.size && !intentionallyEmpty) return preset("default");
  const focus = dockAreas.find((a) => a === root.focus && (a === "main" || groups[a].active)) ?? "main";
  const sizes = object(root.sizes);
  const reopen: Layout["reopen"] = {};
  for (const [id, value] of Object.entries(object(root.reopen) ?? {})) {
    const area = dockAreas.find((a) => a === value);
    if (area && isPanel(id) && !seen.has(id)) reopen[id] = area;
  }
  return { version: 1, groups, focus, reopen, sizes: { right: clampSize("right", sizes?.right), bottom: clampSize("bottom", sizes?.bottom) } };
}

export function readLayout(): Layout {
  try {
    const raw = window.localStorage.getItem(layoutKey);
    if (raw && raw.length <= 8192) return restoreLayout(JSON.parse(raw));
  } catch { /* An unavailable or damaged preference must not prevent startup. */ }
  return preset("default");
}

export function saveLayout(layout: Layout) {
  try { window.localStorage.setItem(layoutKey, JSON.stringify(layout)); }
  catch { /* Still usable without saved preferences. */ }
}

export const locationOf = (layout: Layout, id: PanelId) => dockAreas.find((a) => layout.groups[a].ids.includes(id));
const home = (id: PanelId): DockArea => id === "activity" ? "bottom" : "main";

function without(entry: DockGroup, id: PanelId): DockGroup {
  const index = entry.ids.indexOf(id);
  if (index < 0) return entry;
  const ids = entry.ids.filter((p) => p !== id);
  return group(ids, entry.active === id ? ids[Math.min(index, ids.length - 1)] ?? null : entry.active);
}

export type DockAction =
  | { kind: "open"; id: PanelId }
  | { kind: "close"; id: PanelId }
  | { kind: "move"; id: PanelId; area: DockArea; before?: PanelId }
  | { kind: "focus"; area: DockArea }
  | { kind: "resize"; area: SplitArea; value: number }
  | { kind: "preset"; preset: LayoutPreset };

function update(layout: Layout, action: DockAction): Layout {
  switch (action.kind) {
    case "focus":
      return action.area === layout.focus ? layout : { ...layout, focus: action.area };
    case "resize": {
      const value = clampSize(action.area, action.value);
      return layout.sizes[action.area] === value ? layout : { ...layout, sizes: { ...layout.sizes, [action.area]: value } };
    }
    case "preset": return preset(action.preset);
    case "close": {
      const area = locationOf(layout, action.id);
      if (!area) return layout;
      const groups = { ...layout.groups, [area]: without(layout.groups[area], action.id) };
      const focus = groups[layout.focus].active ? layout.focus : dockAreas.find((a) => groups[a].active) ?? "main";
      return { ...layout, groups, focus, reopen: { ...layout.reopen, [action.id]: area } };
    }
    case "open": {
      if (!isPanel(action.id)) return layout;
      const area = locationOf(layout, action.id);
      if (!area) return update(layout, { kind: "move", id: action.id, area: layout.reopen[action.id] ?? home(action.id) });
      const entry = layout.groups[area];
      if (entry.active === action.id && layout.focus === area) return layout;
      return { ...layout, focus: area, groups: { ...layout.groups, [area]: group(entry.ids, action.id) } };
    }
    case "move": {
      if (!isPanel(action.id)) return layout;
      if (action.before === action.id && locationOf(layout, action.id) === action.area) return update(layout, { kind: "open", id: action.id });
      const from = locationOf(layout, action.id);
      if (from === action.area && action.before === undefined && layout.groups[from].ids.at(-1) === action.id) {
        return update(layout, { kind: "open", id: action.id });
      }
      const groups = { ...layout.groups };
      if (from) groups[from] = without(groups[from], action.id);
      const ids = [...groups[action.area].ids];
      const index = action.before ? ids.indexOf(action.before) : -1;
      ids.splice(index < 0 ? ids.length : index, 0, action.id);
      groups[action.area] = group(ids, action.id);
      const reopen = { ...layout.reopen };
      delete reopen[action.id];
      return { ...layout, groups, reopen, focus: action.area };
    }
  }
}

export type WorkspaceState = { layout: Layout; initialized: ReadonlySet<PanelId> };
export function workspaceState(layout: Layout, previous: ReadonlySet<PanelId> = new Set()): WorkspaceState {
  const initialized = new Set(previous);
  for (const area of dockAreas) {
    const id = layout.groups[area].active;
    if (id) initialized.add(id);
  }
  return { layout, initialized };
}

export function dockReducer(state: WorkspaceState, action: DockAction): WorkspaceState {
  const next = update(state.layout, action);
  return next === state.layout ? state : workspaceState(next, state.initialized);
}
