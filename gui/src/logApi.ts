import { invoke, listen } from "./bridge";

export interface LogLine {
  t: number;
  level: "info" | "warn" | "error";
  msg: string;
}

export const logApi = {
  get: () => invoke<LogLine[]>("get_log"),
  clear: () => invoke<void>("clear_log"),
  isOpen: () => invoke<boolean>("log_is_open"),
  /** Slide the log window out or back behind the main window. */
  setOpen: (open: boolean) =>
    invoke<void>("set_log_open", { open, animate: !window.matchMedia("(prefers-reduced-motion: reduce)").matches }),
  onLine: (cb: (l: LogLine) => void) => listen<LogLine>("log", (e) => cb(e.payload)),
  onOpen: (cb: (open: boolean) => void) => listen<boolean>("log-open", (e) => cb(e.payload)),
};
