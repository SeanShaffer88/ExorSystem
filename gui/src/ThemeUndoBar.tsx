import { useEffect, useState } from "react";
import { themes, undoTheme, useThemeState } from "./theme";

/** Temporary Undo shown after a theme is applied; disappears when time runs out. */
export function ThemeUndoBar() {
  const { undo } = useThemeState();
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!undo) return;
    setNow(Date.now());
    const i = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(i);
  }, [undo]);

  if (!undo) return null;
  const left = Math.max(0, Math.ceil((undo.expiresAt - now) / 1000));
  const prev = themes.find((t) => t.id === undo.previousId)?.name ?? "previous theme";
  return (
    <div className="undo-bar" role="status">
      <span>Theme applied.</span>
      <button type="button" onClick={undoTheme} title={`Go back to ${prev}`}>
        Undo <span aria-hidden="true">({left}s)</span>
      </button>
    </div>
  );
}
