import type { UninstallPreview } from "../types";
import { Badge } from "./Workspace";
import { Icon } from "./Icon";

export function TerminalPanel({ preview, message }: { preview: UninstallPreview | null; message?: string }) {
  return <section className="terminal-panel" aria-label="Read-only execution preview">
    <div className="terminal-header">
      <span className="terminal-dots" aria-hidden="true"><i/><i/><i/></span>
      <span><Icon name="terminal" size={16}/> Vendor invocation</span><Badge tone="safe">Preview only</Badge>
    </div>
    <div className="terminal-body">
      <div className="terminal-comment"># Direct process invocation · no shell · no elevation</div>
      {preview ? <dl className="terminal-args">
        <dt>executable</dt><dd>{preview.executable}</dd>
        <dt>arguments</dt><dd>{preview.args.length ? <ol aria-label="Arguments in order">{preview.args.map((arg, i) => <li key={i}><code>{JSON.stringify(arg)}</code></li>)}</ol> : "[]"}</dd>
      </dl> : <p role="status">{message ?? "Select an application to see its registered invocation."}</p>}
      <div className="terminal-comment"># This is a plan, not a command prompt. Nothing has been executed by this preview.</div>
    </div>
  </section>;
}
