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
  status: () => invoke<{ last: LogLine | null; cleared_through: number }>("log_status"),
  get: () => invoke<LogSnapshot>("get_log"),
  clear: () => invoke<number>("clear_log"),
  onClear: (cb: (through: number) => void) => listen<number>("log-cleared", (e) => cb(e.payload)),
  isOpen: () => invoke<boolean>("log_is_open"),
  /** Omit the value to toggle on the native UI thread, without a stale visibility read. */
  setOpen: (open?: boolean) => invoke<void>("set_log_open", { open }),
  onLine: (cb: (l: LogLine) => void) => listen<LogLine>("log", (e) => cb(e.payload)),
  onOpen: (cb: (open: boolean) => void) => listen<boolean>("log-open", (e) => cb(e.payload)),
};
