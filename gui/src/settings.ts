const KEY = "exor.policy";
const FALLBACK = "standard";

/** The cleanup policy chosen in Settings. Used by every preview and quarantine. */
export function getPolicy(): string {
  try {
    return localStorage.getItem(KEY) || FALLBACK;
  } catch {
    return FALLBACK; // storage can be unavailable
  }
}

export function setPolicy(name: string) {
  try {
    localStorage.setItem(KEY, name);
  } catch {
    /* light per-user convenience only */
  }
}
