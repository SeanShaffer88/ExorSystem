// Vite development only. Synthetic in-memory responses; no native command, registry or file access.
import { installFixture } from "./bridge";
import type { LogLine } from "./logApi";
import type { AppInfo, BatchPreview, Decision, GraphNode, HistoryRow, InstalledProgram, LeftoverReview, PlanHeader, PlanRequest, ProgramRow, QuarantineRow, RelationshipGraph, Session, SessionItems, UninstallPreview, WatchArchiveFilters, WatchArchivePage, WatchArchiveRow, WatchRun, WatchStart, WatchSummary, WatchSystem } from "./types";

const MiB = 1024 ** 2;
const KiB = 1024;
const utcText = () => new Date().toISOString().slice(0, 19).replace("T", " ");
const codeRoot = String.raw`C:\Program Files\Microsoft VS Code`;
const codeExe = `${codeRoot}\\Code.exe`;
const updater = `${codeRoot}\\resources\\app\\update.json`;
const registryPath = String.raw`HKLM\Software\Microsoft\VSCode`;
const types: InstalledProgram["source"][] = ["registry", "msix", "portable"];
const programs = [
  ["Visual Studio Code", "Microsoft", "1.118.0", 388, 0], ["Obsidian", "Obsidian", "1.10.2", 256, 0],
  ["7-Zip", "Igor Pavlov", "26.01", 6.7, 0], ["Microsoft PowerToys", "Microsoft", "0.99.0", 294, 0],
  ["Windows Terminal", "Microsoft", "1.24.15", 73.8, 1], ["Blender", "Blender Foundation", "5.3.0", 733, 0],
  ["Git", "The Git Development Community", "2.54.1", 366, 0], ["Audacity", "Muse Group", "3.9.0", 111, 0],
  ["Notepad++", "Don Ho", "8.9.2", 19.5, 0], ["Calculator", "Microsoft", "11.1", 14.8, 1],
  ["VLC media player", "VideoLAN", "3.0.24", 163, 0], ["KeePass Portable", "Dominik Reichl", "2.61", null, 2],
] as const;

function demoPrograms(): ProgramRow[] {
  return programs.map(([name, publisher, version, size, source], i) => {
    const root = i === 0 ? codeRoot : `C:\\Program Files\\${name}`;
    return { program: {
      id: `demo:${i}`, name, publisher, version, source: types[source]!,
      install_location: source === 1 ? null : root, size_kb: size === null ? null : Math.round(size * KiB),
      scope: i === 1 || source !== 0 ? "user" : "machine", system_component: false, parent_key: null, windows_installer: false,
      uninstall_command: source === 0 ? `"${root}\\unins000.exe"` : null, quiet_uninstall_command: null,
      install_date: `202611${String(18 - i).padStart(2, "0")}`,
    }, removed: false, inferred_session_ids: [], session_ids: i === 0 ? [18] : i === 3 ? [16] : i === 5 ? [15] : [] };
  });
}

function session(session_id: number, app_name: string, status: Session["status"] = "completed"): Session {
  return { session_id, app_name, status, started_at: null, ended_at: null, fingerprint_family: null, installer_path: null };
}
function observation(id: number, path: string, domain: HistoryRow["domain"], what: string, session_id: number | null, time: string, old_hash: string | null = null, new_hash: string | null = null): HistoryRow {
  return { id, path, domain, what, session_id, when: `2026-11-18 ${time}:00`, source: what === "quarantined" || what === "restored" ? "policy" : "snapshot_diff", old_hash, new_hash };
}
const beforeHash = "21f74680aac237d28de17c5eb38726a4fc9ed0155d8bb76b1e3b36fc94fae26a";
const afterHash = "6cfe2c6febc11ad8b20ae7afd308b64ed41d8f2d42c026db56725076c57471fc";

const initialRows = [
  observation(36, `${registryPath}\\Capabilities`, "registry", "created", 18, "06:16"),
  observation(35, updater, "file", "modified", 18, "06:16", beforeHash, afterHash),
  observation(34, String.raw`HKCU\Software\Microsoft\VSCode\LegacyUpdater`, "registry", "deleted", 18, "06:15"),
  observation(33, codeExe, "file", "created", 18, "06:15", null, afterHash),
  observation(32, String.raw`C:\ProgramData\ExorDemo\stale-cache.bin`, "file", "quarantined", null, "05:50", beforeHash),
  observation(31, registryPath, "registry", "modified", 18, "06:15"),
  observation(30, String.raw`task:\Microsoft\VSCode\Update`, "system", "created", 18, "06:15"),
  observation(29, String.raw`service:CodeUpdate`, "system", "created", 18, "06:15"),
  observation(28, `${codeRoot}\\cache-old.bin`, "file", "deleted", 18, "06:15", beforeHash),
  observation(27, String.raw`HKLM32\Software\7-Zip`, "registry", "modified", 17, "05:44"),
  observation(26, String.raw`HKCU\Software\Microsoft\PowerToys`, "registry", "created", 16, "05:25"),
];
const node = (id: number, path: string, kind: GraphNode["kind"] = "file", session_name: GraphNode["session_name"] = null): GraphNode => ({ id, path, kind, session_name });
const graphNodes = [node(1, updater, "file", "Visual Studio Code"), node(2, String.raw`C:\Installers\VSCodeSetup-x64.exe`, "file", "Visual Studio Code setup"),
  node(3, codeExe, "file", "Visual Studio Code"), node(4, "service:CodeUpdate", "service", "Updater service"), node(5, String.raw`task:\Microsoft\VSCode\Update`, "task", "Update task"),
  node(6, `${codeRoot}\\resources\\app\\runtime.dll`, "file", "Shared runtime")];
const links: [number, number, RelationshipGraph["links"][number]["edge"]["relationship_type"], number][] = [
  [2, 1, "modified_by", .3], [2, 3, "created_by", .3], [1, 4, "depends_on", .7], [3, 4, "depends_on", .7], [1, 5, "depends_on", .7], [1, 6, "shared_with", .7],
];
const templates = [
  { name: "conservative", description: "Only quarantine files confirmed by a manifest; everything uncertain goes to review." },
  { name: "standard", description: "Quarantine files inferred or confirmed; review the rest." },
  { name: "thorough", description: "Quarantine files attributed to the session; protected items stay." },
];

export function startDemo() {
  const rows = demoPrograms();
  const sessions = [
    { ...session(22, "Example Editor (cancelled)", "failed"), started_at: "2026-11-18 07:29:00", ended_at: "2026-11-18 07:31:00" },
    { ...session(21, "Windows Terminal"), started_at: "2026-11-18 07:00:00", ended_at: "2026-11-18 07:10:00" },
    { ...session(20, "Example Editor (skipped)", "failed"), started_at: "2026-11-18 06:50:00", ended_at: "2026-11-18 06:55:00" },
    session(19, "Interrupted Watch Example", "failed"),
    { ...session(18, "Visual Studio Code"), started_at: "2026-11-18 06:11:00", ended_at: "2026-11-18 06:16:00" },
    session(17, "7-Zip"), session(16, "Microsoft PowerToys"), session(15, "Blender"),
    ...programs.map(([name], i) => session(14 - i, name)),
    session(2, "Example Editor (archived)"), session(1, "Classic Editor 1.0", null),
  ];
  const observations = [...initialRows];
  const scope = (requested: number, usable = requested) => ({ requested, usable });
  const zero = { files_changed: 0, registry_keys_changed: 0, services_changed: 0, tasks_changed: 0 };
  const folderCoverage = (usable: number) => ({ folders: scope(1, usable), registry: scope(0), services: scope(0), tasks: scope(0) });
  const comparisons = new Map<number, WatchSummary>([[18, {
    session_id: 18,
    coverage: { folders: scope(2, 1), registry: scope(2), services: scope(1), tasks: scope(1) },
    files_changed: 3, registry_keys_changed: 3, services_changed: 1, tasks_changed: 1,
    warnings: ["Demo: one folder was not captured after changes because access was denied."],
  }], [21, { ...zero, session_id: 21, coverage: folderCoverage(1), warnings: [] }],
  [20, { ...zero, session_id: 20, coverage: folderCoverage(0), warnings: ["Demo: the folder could not be read after changes."] }]]);
  const batches: QuarantineRow[] = [
    { quarantine_id: "q19ab5-0004", session_id: 18, files: 19, file_bytes: 264 * MiB, status: "quarantined" },
    { quarantine_id: "q19a71-0003", session_id: 15, files: 88, file_bytes: 1311 * MiB, status: "purged" },
    { quarantine_id: "q19a6f-0002", session_id: 16, files: 8, file_bytes: 148 * MiB, status: "quarantined" },
    { quarantine_id: "q19a20-0001", session_id: null, files: 3, file_bytes: 76 * MiB, status: "restored" },
  ];
  const portable = new Map([[String.raw`C:\Apps\KeePass\KeePass.exe`, "KeePass Portable"]]);
  const logs: LogLine[] = [];
  const subscriptions = new Map<string, Set<(payload: unknown) => void>>();
  let active: { baseline: WatchStart; folder?: string; registry?: string } | null = null;
  let capture: { id: unknown; stopped: boolean } | null = null;
  const captureAfter = async (args: Record<string, unknown>, ms: number) => {
    if (capture) throw new Error("Another capture is already in progress.");
    const attempt = { id: args.captureId, stopped: false };
    capture = attempt;
    try { (args.onReady as (e: { event: "ready" }) => void)({ event: "ready" }); await wait(ms); return attempt.stopped; }
    finally { capture = null; }
  };
  const closeSession = (id: number, status: Session["status"]) => {
    const item = sessions.find((s) => s.session_id === id);
    if (item) { item.status = status; item.ended_at = utcText(); }
  };
  const matchingRows = (args: Record<string, unknown>) => {
    const filter = typeof args.filter === "string" ? args.filter.toLowerCase() : "";
    return observations.filter((r) => (!args.domain || r.domain === args.domain) && (args.sessionId == null || r.session_id === args.sessionId)
      && (!args.changeKind || r.what === args.changeKind)
      && (typeof args.fromDate !== "string" || (r.when ?? "") >= args.fromDate)
      && (typeof args.beforeDate !== "string" || (r.when ?? "") < args.beforeDate)
      && (typeof args.beforeId !== "number" || r.id < args.beforeId) && r.path.toLowerCase().includes(filter));
  };
  let sequence = 0;
  let clearedThrough = 0;
  const updateLog = (msg: string) => {
    const line: LogLine = { seq: ++sequence, t: Date.now(), level: "info", msg };
    logs.push(line);
    if (logs.length > 1000) logs.shift();
    for (const callback of subscriptions.get("log") ?? []) callback(line);
  };
  const moveFixture = (session_id: number | null, files: number, file_bytes: number) => {
    const row = { quarantine_id: `demo-${batches.length + 1}`, session_id, files, file_bytes, status: "quarantined" };
    batches.unshift(row);
    const summary = `Demo: moved ${files} files to quarantine. No real files were accessed.`;
    updateLog(summary);
    return { quarantine_id: row.quarantine_id, moved: files, skipped: 1, summary };
  };
  updateLog("Demo workspace · no access to your files or registry");

  const findProgram = (id: unknown) => rows.find((r) => r.program.id === id);
  const invocationFor = (id: unknown): UninstallPreview => {
    const row = findProgram(id);
    if (!row || row.removed || row.program.source !== "registry" || row.program.scope === "user") throw new Error("This is not a supported machine-wide application.");
    return { executable: `${row.program.install_location}\\unins000.exe`, args: ["/NORESTART"] };
  };
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  let nextPlanId = 1;
  const plans = new Map<number, { plan: PlanHeader; sessionId: number | null; read: boolean }>();
  const foldersFor = (id: unknown) => {
    const root = findProgram(id)?.program.install_location ?? codeRoot;
    return [
      { path: `${root}\\cache`, reason: "An application folder matches this registration.", confidence: .7, can_quarantine: true },
      { path: String.raw`D:\Backups\Setup`, reason: "Reported backup location.", confidence: .3, can_quarantine: false },
    ];
  };
  installFixture({
    listen<T>(event: string, callback: (event: { payload: T }) => void) {
      const listeners = subscriptions.get(event) ?? new Set();
      subscriptions.set(event, listeners);
      const wrap = (payload: unknown) => callback({ payload: payload as T });
      listeners.add(wrap);
      return () => { listeners.delete(wrap); };
    },
    async invoke(command, args) {
      switch (command) {
        case "get_log": return { cleared_through: clearedThrough, lines: [...logs] };
        case "clear_log": {
          logs.length = 0;
          clearedThrough = sequence;
          for (const callback of subscriptions.get("log-cleared") ?? []) callback(clearedThrough);
          return clearedThrough;
        }
        case "app_info": return { version: "0.3.0", data_folder: String.raw`C:\Users\Demo\AppData\Local\ExorSystem`, quarantine_folder: String.raw`C:\Users\Demo\AppData\Local\ExorSystem\quarantine`, legacy_data_folder: null, recovery_warnings: [] } satisfies AppInfo;
        case "list_templates": return templates;
        case "list_programs": return { programs: [
          ...rows.filter((r) => r.program.source !== "portable"),
          ...[...portable].map(([executable, name]) => ({ program: { ...rows[0]!.program, source: "portable", id: `portable:${executable}`, scope: "user", name, version: null, publisher: null, size_kb: null, install_location: executable, uninstall_command: null, install_date: null }, removed: false, inferred_session_ids: [], session_ids: [] })),
        ].map((r) => ({ ...r })), warnings: [] };
        case "list_sessions": return sessions.slice(0, 150);
        case "watch_archive": {
          const { search, status, beforeId } = args.filters as WatchArchiveFilters;
          const term = search?.trim() ?? "";
          if (new TextEncoder().encode(term).length > 240 || /[\u0000-\u001f\u007f-\u009f]/.test(term)) {
            throw new Error("Enter at most 240 bytes without control characters.");
          }
          const ascii = (value: string) => value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
          const byId = /^#\d+$/.test(term);
          const id = byId ? BigInt(term.slice(1)) : null;
          if (id !== null && (id <= 0n || id > 9223372036854775807n)) throw new Error("Expected a positive session ID after #.");
          const matches = sessions.filter((s) => (status === null || s.status === status)
            && (beforeId === null || s.session_id < beforeId)
            && (id === null ? ascii(s.app_name).includes(ascii(term)) : BigInt(s.session_id) === id));
          const rows: WatchArchiveRow[] = matches.slice(0, 20).map(({ session_id, app_name, started_at, ended_at, status }) => {
            const summary = comparisons.get(session_id);
            const comparison = summary ? { coverage: summary.coverage, recorded_changes: Object.values(summary.coverage).some((s) => s.usable)
              ? (summary.coverage.folders.usable ? summary.files_changed : 0) + (summary.coverage.registry.usable ? summary.registry_keys_changed : 0)
                + (summary.coverage.services.usable ? summary.services_changed : 0) + (summary.coverage.tasks.usable ? summary.tasks_changed : 0) : null } : null;
            return { session_id, app_name, started_at, ended_at, status, comparison };
          });
          return { rows, next_before_id: matches.length > 20 ? rows.at(-1)!.session_id : null } satisfies WatchArchivePage;
        }
        case "watch_comparison": return comparisons.get(Number(args.sessionId)) ?? null;
        case "recent_watch_runs": return sessions.flatMap((s) => {
          const summary = comparisons.get(s.session_id);
          return summary ? [{ summary, app_name: s.app_name, ended_at: s.ended_at } satisfies WatchRun] : [];
        }).slice(0, 6);
        case "history": {
          const matches = matchingRows(args.filters as Record<string, unknown>);
          const size = Number(args.limit);
          return { rows: matches.slice(0, size), next_before_id: matches.length > size ? matches[size - 1]?.id : null };
        }
        case "save_changes": {
          const records = matchingRows(args.filters as Record<string, unknown>).length;
          return { path: "", records: Math.min(records, 1000), truncated: records > 1000, next_before_id: null };
        }
        case "session_items": {
          const items = [...new Map(observations.filter((r) => r.session_id === args.sessionId && r.what !== "deleted").map((r) => [r.path, { path: r.path, node_type: r.domain === "registry" ? "registry_key" : r.domain === "file" ? "file" : r.path.startsWith("service:") ? "service" : "task", confidence: r.path === updater ? .7 : .3, shared: r.path === codeExe }])).values()]
            .sort((a, b) => a.path.localeCompare(b.path));
          const filter = String(args.filter ?? "").toLowerCase();
          const matching = items.filter((item) => item.path.toLowerCase().includes(filter));
          const offset = Number(args.offset);
          return { total: items.length, matching: matching.length, items: matching.slice(offset, offset + 12) } satisfies SessionItems;
        }
        case "preview_uninstaller": return invocationFor(args.programId);
        case "run_uninstaller": {
          const live = invocationFor(args.programId);
          const expected = args.expected as UninstallPreview | undefined;
          if (live.executable !== expected?.executable || JSON.stringify(live.args) !== JSON.stringify(expected?.args)) throw new Error("The registered uninstaller changed.");
          await wait(650);
          const row = findProgram(args.programId)!;
          row.removed = true;
          updateLog(`Demo: simulated the vendor exit for ${row.program.name}.`);
          return { exit_code: 0, summary: "Demo vendor process finished. No Windows process was launched." };
        }
        case "find_leftovers": return { folders: foldersFor(args.programId), system_entries: [{ kind: "registry_key", path: registryPath, reason: "Key name matches the recorded publisher and product." }], warnings: [] } satisfies LeftoverReview;
        case "cleanup_folders": return foldersFor(args.programId);
        case "link_cleanup_session": {
          const row = findProgram(args.programId);
          const id = Number(args.sessionId);
          if (!row || !sessions.some((s) => s.session_id === id && s.status === "completed")) throw new Error("Choose a completed session and registration.");
          if (!row.session_ids.includes(id)) row.session_ids.push(id);
          return;
        }
        case "prepare_cleanup": {
          const req = args.request as PlanRequest;
          const roots = req.folders.filter((path) => foldersFor(req.programId).some((f) => f.can_quarantine && f.path === path));
          if (!roots.length || roots.length !== req.folders.length) throw new Error("Folder no longer eligible.");
          const plan = { id: nextPlanId++, roots, files: 2, bytes: 8 * KiB, excluded: 1, expires_in_seconds: 600 };
          plans.set(plan.id, { plan, sessionId: req.sessionId, read: false });
          return plan;
        }
        case "cleanup_plan_page": {
          const entry = plans.get(Number(args.id));
          if (!entry) throw new Error("Plan expired.");
          if (args.excluded) return { offset: 0, total: 1, next_offset: null, files: [], exclusions: [{ path: registryPath, reason: "System entries are advisory." }] };
          entry.read = true;
          return { offset: 0, total: 2, next_offset: null, files: [1, 2].map((n) => ({ path: `${entry.plan.roots[0]}\\cache-${n}.bin`, size: 4 * KiB })), exclusions: [] };
        }
        case "apply_cleanup": {
          const id = Number(args.id);
          const entry = plans.get(id);
          if (!entry?.read || args.approvedFiles !== entry.plan.files) throw new Error("Review the whole plan first.");
          plans.delete(id);
          return moveFixture(entry.sessionId, entry.plan.files, entry.plan.bytes);
        }
        case "discard_cleanup_plan": plans.delete(Number(args.id)); return;
        case "stop_cleanup_preflight": return false;
        case "list_quarantines": {
          const offset = Number(args.offset), end = offset + 40;
          return { rows: batches.slice(offset, end).map((b) => ({ ...b })), next_offset: end < batches.length ? end : null };
        }
        case "pick_path": return null;
        case "storage_usage": return { database_bytes: 4 * MiB, wal_bytes: 128 * KiB, manifests_bytes: 12 * KiB, held_bytes: batches.filter((b) => b.status === "quarantined").reduce((n, b) => n + (b.file_bytes ?? 0), 0), migration_backup_bytes: 0, other_bytes: 0, incomplete: false };
        case "backup_profile": case "restore_profile_copy": case "rotate_archived_batch": throw new Error("Browser demo only. No profile or archive files are available.");
        case "quarantine_batch": {
          const batch = batches.find((b) => b.quarantine_id === args.quarantineId);
          if (!batch) throw new Error("Unknown demo batch.");
          const offset = Number(args.offset);
          const length = Math.max(0, Math.min(24, batch.files - offset));
          const size = Math.floor((batch.file_bytes ?? 0) / batch.files);
          return { status: batch.status, created_unix_ms: Date.parse("2026-11-18T05:50:00Z"), total_files: batch.files,
            files: Array.from({ length }, (_, i) => ({ original_path: `C:\\ExorDemo\\${batch.quarantine_id}\\cache-${offset + i + 1}.bin`, size })),
            next_offset: offset + length < batch.files ? offset + length : null } satisfies BatchPreview;
        }
        case "restore_quarantine":
        case "purge_quarantine": {
          const batch = batches.find((b) => b.quarantine_id === args.quarantineId && b.status === "quarantined");
          if (!batch) throw new Error("Batch is not held.");
          batch.status = command === "restore_quarantine" ? "restored" : "purged";
          updateLog(`Demo: ${batch.status} ${batch.files} synthetic files.`);
          return batch.files;
        }
        case "simulate": {
          const isCode = args.sessionId === 18;
          const decisions: Decision[] = isCode ? [
            { path: updater, node_type: "file", action: args.template === "conservative" ? "flag_for_review" : "quarantine", reason: "Timing evidence associates this file with the session.", rule: null },
            { path: codeExe, node_type: "file", action: "keep", reason: "Another item depends on this component.", rule: null },
            { path: registryPath, node_type: "registry_key", action: "flag_for_review", reason: "System items are review-only.", rule: null },
          ] : [];
          return { total: decisions.length, next_offset: null, summary: "Synthetic policy decisions", keep: decisions.filter((d) => d.action === "keep").length, quarantine: decisions.filter((d) => d.action === "quarantine").length, flag_for_review: decisions.filter((d) => d.action === "flag_for_review").length, decisions };
        }
        case "provenance": {
          const path = String(args.path);
          const focus = graphNodes.find((n) => n.path.toLowerCase() === path.toLowerCase());
          if (!focus) return { found: false, summary: "This path is not in the demo relationship record.", confidence: 0, graph: null, ledger: [] };
          const edges = links.flatMap(([parent, child, relationship_type, confidence], i) => parent === focus.id || child === focus.id ? [{
            node: graphNodes.find((n) => n.id === (parent === focus.id ? child : parent))!,
            edge: { edge_id: i + 1, parent_node_id: parent, child_node_id: child, relationship_type, confidence_score: confidence, evidence_source: confidence > .3 ? "temporal" : "heuristic" },
          }] : []);
          const ledger = observations.filter((r) => r.path === focus.path).map((r) => ({ entry_id: r.id, observed_at: r.when, entry: { path: r.path, change_kind: r.what, source: r.source, old_hash: r.old_hash, new_hash: r.new_hash } })).reverse();
          return { found: true, summary: `${focus.path} has ${edges.length} recorded relationship(s) in this demo.`, confidence: .3, graph: { focus, links: edges, total_edges: edges.length }, ledger };
        }
        case "begin_watch": {
          if (active || capture) throw new Error("A watch is already open.");
          const request = args.request as { appName: string; folders: string[]; registrySubtrees: string; system: WatchSystem };
          const { folders, system } = request;
          const registry = request.registrySubtrees.split("\n").map((p) => p.trim()).filter(Boolean);
          const warnings = system.tasks ? ["Demo: scheduled tasks were not checked because access was denied."] : [];
          const coverage = { folders: scope(folders.length), registry: scope(registry.length), services: scope(Number(system.services)), tasks: scope(Number(system.tasks), 0) };
          const id = sessions[0]!.session_id + 1;
          sessions.unshift({ ...session(id, request.appName, "monitoring"), started_at: utcText() });
          if (await captureAfter(args, 800)) { closeSession(id, "failed"); updateLog("Demo: stopped the baseline."); return null; }
          if (!Object.values(coverage).some((entry) => entry.usable)) {
            closeSession(id, "failed");
            throw new Error("No requested scopes could be captured in the demo. Choose a folder, registry subtree, or services.");
          }
          const baseline = { session_id: id, coverage, warnings };
          active = { baseline, folder: folders[0], registry: registry[0] };
          return baseline;
        }
        case "finish_watch": {
          if (!active) throw new Error("No active watch.");
          const { baseline, folder, registry } = active;
          active = null;
          if (await captureAfter(args, 800)) {
            closeSession(baseline.session_id, "failed"); updateLog("Demo: stopped the comparison."); return null;
          }
          closeSession(baseline.session_id, "completed");
          let id = observations[0]!.id + 1;
          if (folder) observations.unshift(observation(id++, `${folder.replace(/[\\/]+$/, "")}\\demo-artifact.txt`, "file", "modified", baseline.session_id, "06:20", beforeHash, afterHash));
          if (registry) observations.unshift(observation(id, `${registry}\\DemoKey`, "registry", "created", baseline.session_id, "06:20"));
          updateLog(`Demo: recorded a snapshot comparison (#${baseline.session_id}).`);
          const summary: WatchSummary = { ...baseline, files_changed: folder ? 1 : 0, services_changed: 0, tasks_changed: 0, registry_keys_changed: registry ? 1 : 0 };
          comparisons.set(summary.session_id, summary);
          return summary;
        }
        case "stop_watch_capture":
          if (!capture || capture.id !== args.captureId) return false;
          capture.stopped = true; return true;
        case "cancel_watch": if (active) closeSession(active.baseline.session_id, "failed"); active = null; return;
        case "list_portable_programs": return [...portable].map(([executable, name]) => ({ executable, name }));
        case "register_portable_program": {
          if (!/^[A-Za-z]:\\.*\.exe$/i.test(String(args.executable))) throw new Error("Enter a full Windows .exe path.");
          portable.set(String(args.executable), String(args.name)); return;
        }
        case "forget_portable_program": portable.delete(String(args.executable)); return;
        case "update_risk": return { not_assessed: [], low: [{ name: "7-Zip", why: "No recorded service or task changes." }], medium: [{ name: "Visual Studio Code", why: "A shared file appears in recorded evidence." }], high: [] };
        case "scan": return { event: "done", data: { files: 14, skipped: 0, root_digest: afterHash } };
        case "cancel_scan": return;
        case "export_report": return "Demo preview only (no file saved)";
        case "make_portable": return { summary: "Demo: no portable files were created." };
        default: throw new Error(`This command is not available in the demo: ${command}`);
      }
    },
  });
}
