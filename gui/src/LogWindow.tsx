import { useEffect, useRef, useState } from "react";
import { logApi, type LogLine } from "./logApi";
import { initTheme } from "./theme";

const time = (t: number) => new Date(t).toLocaleTimeString([], { hour12: false });
const word = { info: "INFO", warn: "WARN", error: "FAIL" } as const;

/** Content of the small activity-log window that slides out from behind the main window. */
export function LogWindow() {
  const [lines, setLines] = useState<LogLine[]>([]);
  const box = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    let live = true;
    const un = logApi.onLine((l) => live && setLines((p) => [...p.slice(-999), l]));
    logApi.get().then((l) => live && setLines((p) => (p.length ? p : l))).catch(() => {});
    // the main window saves theme changes; follow them
    const onStorage = () => initTheme();
    window.addEventListener("storage", onStorage);
    return () => {
      live = false;
      un.then((f) => f());
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  useEffect(() => {
    const el = box.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [lines]);

  return (
    <div className="logwin">
      <div className="logbar">
        <strong id="log-h">Activity log</strong>
        <span>
          <button type="button" onClick={() => { void logApi.clear(); setLines([]); }} title="Clear the log">Clear</button>{" "}
          <button type="button" onClick={() => void logApi.setOpen(false)} title="Hide the log (Ctrl+`)">Hide</button>
        </span>
      </div>
      <div
        ref={box}
        className="logbody"
        role="log"
        aria-labelledby="log-h"
        aria-live="off"
        tabIndex={0}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        {lines.length === 0 && <p className="logline">No activity yet.</p>}
        {lines.map((l, i) => (
          <p key={i} className={`logline log-${l.level}`}>
            <span className="log-t">{time(l.t)}</span> <span className="log-l">{word[l.level]}</span> {l.msg}
          </p>
        ))}
      </div>
    </div>
  );
}
