import { invoke, listen } from "./bridge";

export interface LogLine {
  seq: number;
  t: number;
  level: "info" | "warn" | "error";
  msg: string;
}

export interface LogSnapshot {
  cleared_through: number;
  lines: LogLine[];
}

export const logApi = {
  get: () => invoke<LogSnapshot>("get_log"),
  clear: () => invoke<number>("clear_log"),
  onClear: (cb: (through: number) => void) => listen<number>("log-cleared", (e) => cb(e.payload)),
  isOpen: () => invoke<boolean>("log_is_open"),
  /** Slide the log window out or back behind the main window. */
  setOpen: (open: boolean) =>
    invoke<void>("set_log_open", { open, animate: !window.matchMedia("(prefers-reduced-motion: reduce)").matches }),
  onLine: (cb: (l: LogLine) => void) => listen<LogLine>("log", (e) => cb(e.payload)),
  onOpen: (cb: (open: boolean) => void) => listen<boolean>("log-open", (e) => cb(e.payload)),
};
