import { useSyncExternalStore } from "react";
import themesData from "./themes.json";

export interface Theme {
  id: string;
  name: string;
  mode: "dark" | "light";
  description: string;
  tokens: Record<"bg" | "panel" | "fg" | "muted" | "border" | "accent" | "danger", string>;
}

export const themes = themesData as Theme[];
export const DEFAULT_THEME = "default";
/** How long the Undo button stays available after applying a theme. */
export const UNDO_SECONDS = 30;

const KEY = "exor.theme";

interface State {
  applied: string;
  preview: string | null;
  undo: { previousId: string; expiresAt: number } | null;
}

const byId = (id: string) => themes.find((t) => t.id === id);

function load(): string {
  try {
    const id = localStorage.getItem(KEY);
    if (id && byId(id)) return id;
  } catch {
    /* storage can be unavailable; fall back to the default */
  }
  return DEFAULT_THEME;
}

function save(id: string) {
  try {
    localStorage.setItem(KEY, id);
  } catch {
    /* light per-user convenience only */
  }
}

function paint(id: string) {
  const t = byId(id) ?? byId(DEFAULT_THEME);
  if (!t) return;
  const root = document.documentElement;
  for (const [k, v] of Object.entries(t.tokens)) root.style.setProperty(`--${k}`, v);
  root.style.colorScheme = t.mode;
}

let state: State = { applied: DEFAULT_THEME, preview: null, undo: null };
let timer: number | undefined;
const listeners = new Set<() => void>();

function set(next: State) {
  state = next;
  paint(state.preview ?? state.applied);
  listeners.forEach((l) => l());
}

/** Call once before the first render. */
export function initTheme() {
  set({ applied: load(), preview: null, undo: null });
}

/** Show a theme without keeping it. */
export function previewTheme(id: string) {
  if (byId(id)) set({ ...state, preview: id === state.applied ? null : id });
}

export function endPreview() {
  if (state.preview !== null) set({ ...state, preview: null });
}

/** Keep a theme and offer a temporary Undo. */
export function applyTheme(id: string) {
  if (!byId(id) || id === state.applied) return;
  window.clearTimeout(timer);
  const undo = { previousId: state.applied, expiresAt: Date.now() + UNDO_SECONDS * 1000 };
  save(id);
  set({ applied: id, preview: null, undo });
  timer = window.setTimeout(() => set({ ...state, undo: null }), UNDO_SECONDS * 1000);
}

export function undoTheme() {
  if (!state.undo) return;
  window.clearTimeout(timer);
  const prev = state.undo.previousId;
  save(prev);
  set({ applied: prev, preview: null, undo: null });
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useThemeState(): State {
  return useSyncExternalStore(subscribe, () => state);
}
