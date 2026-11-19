import { useEffect } from "react";
import { applyTheme, endPreview, previewTheme, themes, useThemeState } from "../theme";

export function AppearanceView({ visible }: { visible: boolean }) {
  const { applied, preview } = useThemeState();

  // Leaving the active tab (or the compact dock) cancels an unapplied preview.
  useEffect(() => { if (visible) return endPreview; }, [visible]);
  const selected = preview ?? applied;

  const groups = [
    { label: "Dark themes", items: themes.filter((t) => t.mode === "dark") },
    { label: "Light themes", items: themes.filter((t) => t.mode === "light") },
  ];

  return (
    <section aria-labelledby="h-look">
      <h2 id="h-look">Appearance</h2>
      <p>Pick a theme to preview it right away. Nothing is kept until you choose Apply.</p>
      <div className="row">
        <button type="button" disabled={selected === applied} onClick={() => applyTheme(selected)} title="Keep the previewed theme">
          Apply theme
        </button>
        <button
          type="button"
          disabled={selected === applied}
          onClick={endPreview}
          title="Go back to the theme you are using"
        >
          Cancel preview
        </button>
      </div>
      {groups.map((g) => (
        <fieldset key={g.label} className="themes">
          <legend>{g.label}</legend>
          {g.items.map((t) => (
            <label key={t.id} className="theme-option" title={t.description}>
              <input type="radio" name="theme" value={t.id} checked={selected === t.id} onChange={() => previewTheme(t.id)} />
              <span className="swatches" aria-hidden="true">
                {(["bg", "panel", "accent", "fg"] as const).map((k) => (
                  <span key={k} style={{ background: t.tokens[k] }} />
                ))}
              </span>
              <span>
                {t.name}
                {t.id === applied ? " (in use)" : ""}
                <small>{t.description}</small>
              </span>
            </label>
          ))}
        </fieldset>
      ))}
    </section>
  );
}
