import { useSyncExternalStore } from "react";

const KEY = "exor.policy";
const EVENT = "exor-policy-changed";
export const policies = ["conservative", "standard", "thorough"] as const;
export type Policy = typeof policies[number];
function valid(name: string | null): Policy {
  return policies.find((p) => p === name) ?? "standard";
}

let transient: Policy | null = null;
/** Read a validated default, even if a stale or invalid value was persisted. */
export function getPolicy(): Policy {
  try { return transient ?? valid(localStorage.getItem(KEY)); }
  catch { return transient ?? "standard"; }
}

export function setPolicy(name: string) {
  transient = valid(name);
  try { localStorage.setItem(KEY, transient); transient = null; } catch { /* in-memory preference */ }
  window.dispatchEvent(new Event(EVENT));
}

function subscribe(callback: () => void) {
  window.addEventListener(EVENT, callback);
  window.addEventListener("storage", callback);
  return () => { window.removeEventListener(EVENT, callback); window.removeEventListener("storage", callback); };
}

/** Hidden but mounted panels follow the live default until explicitly overridden. */
export function usePolicy() { return useSyncExternalStore(subscribe, getPolicy); }
