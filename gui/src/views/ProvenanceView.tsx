import { useEffect, useState } from "react";
import { api } from "../api";
import { ChangeBadge } from "../components/ChangeEvidence";
import { Icon } from "../components/Icon";
import { Badge, CardHeading, Empty, Intro, evidenceText, leafName, nodeText } from "../components/Workspace";
import type { GraphNode, ProvenanceView as Trace, RelationshipGraph } from "../types";

export function ProvenanceView({ initialPath }: { initialPath: string }) {
  const [path, setPath] = useState(initialPath);
  const [lookup, setLookup] = useState<{ path: string } | null>(initialPath ? { path: initialPath } : null);
  const [response, setResponse] = useState<{ lookup: NonNullable<typeof lookup>; trace?: Trace; error?: string } | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const current = response?.lookup === lookup ? response : null;
  const submit = (text: string) => {
    if (!text.trim()) return;
    setPath(text);
    setLookup({ path: text.trim() });
  };
  useEffect(() => {
    if (initialPath) return;
    let live = true;
    api.history(null, 5, null, null, "file").then(({ rows }) => {
      if (live) setSuggestions([...new Set(rows.map((r) => r.path))].slice(0, 4));
    }).catch(() => {});
    return () => { live = false; };
  }, [initialPath]);
  useEffect(() => {
    if (!lookup) return;
    let live = true;
    api.provenance(lookup.path).then((trace) => { if (live) setResponse({ lookup, trace }); })
      .catch((e: unknown) => { if (live) setResponse({ lookup, error: String(e) }); });
    return () => { live = false; };
  }, [lookup]);
  const trace = current?.trace;
  const ledger = trace?.ledger.slice(-100).reverse() ?? [];

  return <section className="workspace-page" aria-labelledby="trace-title">
    <Intro id="trace-title" kicker="PROVENANCE" title="Dependency relationships" description="Follow recorded links between installers, files, registry keys, services, and tasks."/>
    <section className="card tool-card" aria-label="Trace an artifact">
      <form className="tool-form" onSubmit={(e) => { e.preventDefault(); submit(path); }}>
        <label className="field grow">File path or registry key<input value={path} onChange={(e) => setPath(e.target.value)} placeholder="Enter a full recorded path"/></label>
        <button className="primary" type="submit" disabled={!path.trim()}><Icon name="search"/> Trace</button>
      </form>
      {!lookup && !!suggestions.length && <div className="trace-suggestions"><span className="muted">Recent file observations:</span>{suggestions.map((p) => <button key={p} type="button" className="quiet-button" onClick={() => submit(p)} title={p}>{leafName(p)}</button>)}</div>}
    </section>
    {current?.error && <p role="alert" className="notice review-notice">Could not trace this path: {current.error}</p>}
    {lookup && !current && <div className="card" role="status"><Empty icon="graph" title="Loading relationship evidence…"/></div>}
    {!lookup && <section className="card"><Empty icon="graph" title="Choose an artifact to trace" detail="You can also open a path from the change inspector or an application's recorded evidence."/></section>}
    {trace && <div className="notice" role="status"><Icon name="info"/><span>{trace.summary}</span></div>}
    {trace?.graph && <Graph key={trace.graph.focus.id} graph={trace.graph} onTrace={submit}/>}
    {!!ledger.length && <section className="card card-overflow" aria-label="Artifact history">
      <CardHeading title="Artifact history" subtitle={trace && trace.ledger.length > 100 ? "100 latest observations" : "Recorded observations · newest first"}/>
      <div className="dashboard-scroll"><table className="dashboard-table"><thead><tr><th scope="col">Observed (UTC)</th><th scope="col">Change</th><th scope="col">Evidence source</th></tr></thead><tbody>{ledger.map((l) => <tr key={l.entry_id}><td>{l.observed_at ?? "Unknown"}</td><td><ChangeBadge what={l.entry.change_kind}/></td><td>{evidenceText[l.entry.source] ?? l.entry.source}</td></tr>)}</tbody></table></div>
    </section>}
  </section>;
}

type Link = RelationshipGraph["links"][number];
function Graph({ graph, onTrace }: { graph: RelationshipGraph; onTrace: (p: string) => void }) {
  const origins = graph.links.filter((l) => l.edge.child_node_id === graph.focus.id && l.edge.relationship_type !== "shared_with");
  const dependents = graph.links.filter((l) => !origins.includes(l));
  return <section className="card graph-card" aria-label="Relationship graph">
    <CardHeading title="Relationship graph" subtitle="One hop from the selected artifact · click a node to trace it" action={<Badge tone="violet">{graph.total_edges} recorded link{graph.total_edges === 1 ? "" : "s"}</Badge>}/>
    <div className="graph-canvas">
      <GraphLane title="Origins & dependencies" side="left" links={origins} onTrace={onTrace}/>
      <div className="graph-focus"><div className="eyebrow">Selected artifact</div><span className="tile-icon tone-safe"><Icon name="graph"/></span><GraphIdentity node={graph.focus}/><code>{graph.focus.path}</code></div>
      <GraphLane title="Dependents & shared" side="right" links={dependents} onTrace={onTrace}/>
    </div>
    <p className="graph-note muted">A link reflects evidence in this profile, not an exhaustive dependency list or proof of ownership.{graph.links.length < graph.total_edges ? ` Showing ${graph.links.length} of ${graph.total_edges} recorded links.` : ""}</p>
  </section>;
}

const leftLabel = { created_by: "Focus created by", modified_by: "Focus modified by", depends_on: "Focus depends on", shared_with: "Shared with focus" } as const;
const rightLabel = { created_by: "Created by focus", modified_by: "Modified by focus", depends_on: "Depends on focus", shared_with: "Shared with focus" } as const;
function GraphLane({ title, side, links, onTrace }: { title: string; side: "left" | "right"; links: Link[]; onTrace: (p: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  return <div className={`graph-lane graph-${side}`}>
    <h3>{title}</h3>
    {links.length ? <ul>{(expanded ? links : links.slice(0, 4)).map(({ node, edge }) => (
      <li key={edge.edge_id}>
        <button type="button" className="graph-node" onClick={() => onTrace(node.path)} title={node.path}>
          <span className="graph-edge-label">{(side === "left" ? leftLabel : rightLabel)[edge.relationship_type]}</span><GraphIdentity node={node}/>
          <code>{node.path}</code>
          <span className="graph-confidence">{Math.round(edge.confidence_score * 100)}% confidence{edge.evidence_source ? ` · ${evidenceText[edge.evidence_source] ?? edge.evidence_source}` : ""}</span>
        </button>
      </li>
    ))}</ul> : <p className="muted graph-empty">No links recorded in this direction.</p>}
    {links.length > 4 && <button type="button" className="text-button" onClick={() => setExpanded(!expanded)}>{expanded ? "Show fewer links" : `Show all ${links.length} links`}</button>}
  </div>;
}

function GraphIdentity({ node }: { node: GraphNode }) {
  return <span className="graph-identity"><strong>{node.session_name || leafName(node.path)}</strong><small>{nodeText[node.kind]}{node.session_name ? ` · ${leafName(node.path)}` : ""}</small></span>;
}
