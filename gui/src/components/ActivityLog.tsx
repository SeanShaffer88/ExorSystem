import { useEffect, useReducer, useRef, useState } from "react";
import { logApi, type LogLine, type LogSnapshot } from "../logApi";

function mergeLog(state: LogSnapshot, update: { cleared_through?: number; lines?: LogLine[] }): LogSnapshot {
  const cleared_through = Math.max(state.cleared_through, update.cleared_through ?? 0);
  const unique = new Map<number, LogLine>();
  for (const lines of [state.lines, update.lines ?? []]) {
    for (const line of lines) if (line.seq > cleared_through) unique.set(line.seq, line);
  }
  return { cleared_through, lines: [...unique.values()].sort((a, b) => a.seq - b.seq).slice(-1000) };
}

export function useActivityLog(enabled = true) {
  const [{ lines }, update] = useReducer(mergeLog, { cleared_through: 0, lines: [] });
  const [error, setError] = useState("");
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    const report = (e: unknown) => { if (live) setError(`Activity log unavailable: ${String(e)}`); };
    const listeners = [
      logApi.onLine((line) => { if (live) update({ lines: [line] }); }),
      logApi.onClear((through) => { if (live) update({ cleared_through: through }); }),
    ].map((p) => p.catch((e: unknown) => { report(e); return () => {}; }));
    // Subscribe before fetching. A clear watermark prevents an older snapshot or event reviving a line.
    void Promise.all(listeners).then(async () => {
      if (!live) return;
      const snapshot = await logApi.get();
      if (live) update(snapshot);
    }).catch(report);
    return () => {
      live = false;
      for (const listener of listeners) void listener.then((stop) => stop()).catch(() => {});
    };
  }, [enabled]);

  const clear = async () => {
    if (clearing) return;
    setClearing(true); setError("");
    try { update({ cleared_through: await logApi.clear() }); }
    catch (e) { setError(`Could not clear the activity log: ${String(e)}`); }
    finally { setClearing(false); }
  };

  return { lines, error, clear, clearing };
}

/** Latest status and watermark only; the native main window never loads scrollback. */
export function useLatestLog(enabled: boolean) {
  const [status, update] = useReducer((old: { last: LogLine | null; cleared_through: number }, next: { last?: LogLine | null; cleared_through?: number }) => {
    const cleared_through = Math.max(old.cleared_through, next.cleared_through ?? 0);
    const last = next.last && next.last.seq > (old.last?.seq ?? 0) ? next.last : old.last;
    return { cleared_through, last: last && last.seq > cleared_through ? last : null };
  }, { cleared_through: 0, last: null });
  const [error, setError] = useState("");
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    const listeners = [logApi.onLine((last) => { if (live) update({ last }); }), logApi.onClear((cleared_through) => { if (live) update({ cleared_through }); })];
    void Promise.all(listeners).then(() => logApi.status()).then((snapshot) => { if (live) update(snapshot); })
      .catch((e: unknown) => { if (live) setError(`Activity status unavailable: ${String(e)}`); });
    return () => { live = false; for (const l of listeners) void l.then((stop) => stop()).catch(() => {}); };
  }, [enabled]);
  return { ...status, error };
}

export type ActivityLogState = ReturnType<typeof useActivityLog>;
const time = (t: number) => new Date(t).toLocaleTimeString([], { hour12: false });
const word = { info: "INFO", warn: "WARN", error: "FAIL" } as const;

/** Shared scrollback view for the workspace panel and the attached window. */
export function LogLines({ lines, visible = true }: { lines: LogLine[]; visible?: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useEffect(() => {
    const el = box.current;
    if (el && visible && stick.current) el.scrollTop = el.scrollHeight;
  }, [lines, visible]);
  return <div ref={box} className="logbody" role="log" aria-label="Activity messages" aria-live="off" tabIndex={0}
    onScroll={(e) => {
      const el = e.currentTarget;
      if (visible) stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    }}>
    {!lines.length && <p className="logline muted">No activity yet.</p>}
    {lines.map((l) => <p key={l.seq} className={`logline log-${l.level}`}>
      <span className="log-t">{time(l.t)}</span> <span className="log-l">{word[l.level]}</span> {l.msg}
    </p>)}
  </div>;
}
