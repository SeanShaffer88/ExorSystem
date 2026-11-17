import { useEffect, useState } from "react";
import { api } from "../api";
import { getPolicy, setPolicy } from "../settings";
import type { AppInfo, TemplateInfo } from "../types";

export function SettingsView() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [policy, setChoice] = useState(getPolicy());
  const [error, setError] = useState("");

  useEffect(() => {
    api.listTemplates().then(setTemplates).catch((e: unknown) => setError(String(e)));
    api.appInfo().then(setInfo).catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <section aria-labelledby="h-set">
      <h2 id="h-set">Settings</h2>
      {error && <p role="alert" className="error">{error}</p>}
      <fieldset className="themes">
        <legend>Cleanup policy</legend>
        <p>How cautious previews and quarantine are. Items that may be shared are always kept.</p>
        {templates.map((t) => (
          <label key={t.name} className="theme-option">
            <input type="radio" name="policy" checked={policy === t.name} onChange={() => { setPolicy(t.name); setChoice(t.name); }} />
            <span>{t.name[0]?.toUpperCase()}{t.name.slice(1)}<small>{t.description}</small></span>
          </label>
        ))}
      </fieldset>
      {info && (
        <dl className="facts">
          <dt>Version</dt><dd>{info.version}</dd>
          <dt>Data folder</dt><dd>{info.data_folder}</dd>
          <dt>Quarantine folder</dt><dd>{info.quarantine_folder}</dd>
        </dl>
      )}
    </section>
  );
}
