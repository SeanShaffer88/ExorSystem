// Exercise the production bundle, replacing only the Tauri IPC boundary.
import { readFile } from 'node:fs/promises';
import path from 'node:path';

export async function mockDesktop(page) {
  const dist = path.resolve(import.meta.dirname, '../dist');
  await page.route('https://exor.test/**', async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    const p = path.join(dist, pathname === '/' ? 'index.html' : pathname);
    const contentType = ({'.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css'})[path.extname(p)];
    await route.fulfill({contentType, body: await readFile(p)});
  });
  await page.addInitScript(() => {
    const standard = {
      version: '1.0', publisher: 'Fixture', install_location: 'C:\\Program Files\\Zed Widget',
      uninstall_command: null, quiet_uninstall_command: null, install_date: null, size_kb: null,
      scope: 'user', system_component: false, parent_key: null, windows_installer: false,
    };
    const app = { ...standard, name: 'Zed Widget', size_kb: 4096, source: 'registry', id: 'HKLM\\Zed', scope: 'machine', uninstall_command: 'C:\\Program Files\\Zed Widget\\uninstall.exe' };
    const userApp = { ...app, id: 'HKCU\\Fixture', name: 'Per-user Fixture', scope: 'user', size_kb: null };
    const msix = { ...standard, name: 'Alpha Package', size_kb: 1024, source: 'msix', id: 'msix:AlphaPackage' };
    const moveable = 'C:\\Program Files\\Zed Widget';
    const params = new URLSearchParams(location.search);
    const stressInventory = params.has('inventory-stress');
    const desktopSlices = params.has('desktop-slices');
    const stressRows = stressInventory ? Array.from({ length: 32 }, (_, i) => ({
      program: { ...standard, source: 'registry', id: `stress-${i + 1}`, name: `Inventory Fixture ${i + 1}`,
        install_location: 'C:\\' + 'UnbrokenDirectoryName'.repeat(i === 23 ? 20 : 1) },
      removed: false, session_ids: i === 23 ? [9] : [],
    })) : [];
    let portable = [];
    let removed = false;
    let invocationChanged = true;
    let invocationCount = 0;
    const argv = { executable: `${moveable}\\uninstall.exe`, args: ['--interactive', 'two words'] };
    const batches = [
      { quarantine_id: 'fixture-held', session_id: null, status: 'quarantined', files: desktopSlices ? 27 : 2, file_bytes: 4 * 1024 ** 2 },
      { quarantine_id: 'fixture-deleted', session_id: 9, status: 'purged', files: 3, file_bytes: 100 * 1024 ** 2 },
    ];
    let watch = null;
    let nextWatchId = desktopSlices ? 50 : 40;
    let capture = null;
    const comparisons = new Map();
    const comparisonNames = new Map();
    const counts = { files_changed: 0, services_changed: 0, tasks_changed: 0, registry_keys_changed: 0 };
    const coverage = (folders, files, registry = 0, keys = registry) => ({ folders: { requested: folders, usable: files }, registry: { requested: registry, usable: keys }, services: { requested: 0, usable: 0 }, tasks: { requested: 0, usable: 0 } });
    const ended = '2026-11-18 05:00:00';
    if (desktopSlices) {
      comparisons.set(38, { ...counts, session_id: 38, coverage: coverage(1, 0), warnings: ['A fixture folder was not readable.'] });
      comparisons.set(39, { ...counts, session_id: 39, coverage: coverage(1, 1), warnings: [] });
      comparisons.set(40, { ...counts, session_id: 40, coverage: coverage(2, 1, 1), warnings: ['One folder was skipped.'], files_changed: 2, registry_keys_changed: 2 });
      comparisonNames.set(38, 'Coverage-only Fixture');
      comparisonNames.set(39, 'Zero-change Fixture');
    }
    const failedSessions = [];
    let coverageFailed = desktopSlices;
    let previewFailed = !desktopSlices;
    let restoreFailed = !desktopSlices;
    let historyFailed = false;
    const historyRows = [
      { id: 44, session_id: null, path: String.raw`C:\Unassigned.dat`, what: 'quarantined', source: 'policy' },
      { id: 43, session_id: 9, path: String.raw`C:\Other session.dat`, what: 'created', source: 'manifest' },
      { id: 42, session_id: 40, path: String.raw`HKCU\Software\Watch Fixture\Gone`, what: 'deleted', source: 'snapshot_diff' },
      { id: 41, session_id: 40, path: String.raw`HKCU\Software\Watch Fixture\Changed`, what: 'modified', source: 'snapshot_diff' },
      { id: 40, session_id: 40, path: String.raw`C:\Watch Fixture\Older.txt`, what: 'created', source: 'snapshot_diff' },
      { id: 39, session_id: 40, path: String.raw`C:\Watch Fixture\Modified.txt`, what: 'modified', source: 'snapshot_diff', old_hash: 'a'.repeat(64), new_hash: 'b'.repeat(64) },
    ].map(row => ({ old_hash: null, new_hash: null, ...row, domain: row.path.startsWith('HKCU') ? 'registry' : 'file', when: '2026-11-18 05:00:00' }));
    const calls = [];
    const callbacks = new Map();
    const events = new Map();
    const ready = (args) => {
      const send = callbacks.get(args.onReady.id);
      send({ index: 0, message: null });
      send({ index: 1, end: true });
    };
    const filterHistory = (args) => historyRows.filter(row =>
      (args.domain == null || row.domain === args.domain) &&
      (args.sessionId == null || row.session_id === args.sessionId) &&
      (args.changeKind == null || row.what === args.changeKind) &&
      (args.beforeId == null || row.id < args.beforeId) &&
      (!args.filter || row.path.toLowerCase().includes(args.filter.toLowerCase()))
    );
    let token = 0;
    const emit = (event, payload) => {
      for (const [id, listener] of events) if (listener.event === event) callbacks.get(listener.handler)?.({ event, id, payload });
    };
    window.emitExor = emit;
    const initial = { seq: 1, t: 1_750_000_000_000, level: 'info', msg: 'Fixture started' };
    const entries = [initial];
    let logOpen = false;
    window.setExorLogOpen = (open) => { logOpen = open; emit('log-open', open); };
    let clearedThrough = 0;
    window.appendExorLog = (line) => { entries.push(line); emit('log', line); };
    const delayedLog = new URLSearchParams(window.location.search).has('deferred-log');
    window.exorCalls = calls;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback: (cb) => { const id = ++token; callbacks.set(id, cb); return id; },
      unregisterCallback: (id) => callbacks.delete(id),
      invoke: async (command, args) => {
        calls.push([command, args]);
        switch (command) {
          case 'plugin:event|listen': {
            const id = ++token;
            events.set(id, args);
            return id;
          }
          case 'plugin:event|unlisten': {
            callbacks.delete(events.get(args.eventId)?.handler);
            events.delete(args.eventId);
            return null;
          }
          case 'get_log': {
            const snapshot = { cleared_through: clearedThrough, lines: [...entries] };
            if (delayedLog) await new Promise(resolve => { window.releaseExorLog = resolve; });
            return snapshot;
          }
          case 'clear_log': {
            clearedThrough = Math.max(clearedThrough, ...entries.map(line => line.seq));
            entries.length = 0;
            emit('log-cleared', clearedThrough);
            return clearedThrough;
          }
          case 'log_is_open': return logOpen;
          case 'set_log_open': window.setExorLogOpen(args.open ?? !logOpen); return null;
          case 'scan': {
            const send = callbacks.get(args.onEvent.id);
            send({ index: 0, message: { event: 'progress', data: { phase: 'hashing', files_seen: 6, files_hashed: 2, bytes_hashed: 5 } } });
            await new Promise(resolve => { window.finishExorScan = resolve; });
            send({ index: 1, message: { event: 'done', data: { files: 6, skipped: 0, root_digest: 'b'.repeat(64) } } });
            send({ index: 2, end: true });
            return null;
          }
          case 'update_risk': return { low: [], medium: [], high: [] };
          case 'list_sessions': return [
            ...[...comparisons.values()].reverse().map(({ session_id, coverage }) => ({
              session_id, app_name: comparisonNames.get(session_id) ?? 'Watch Fixture', ended_at: ended,
              status: Object.values(coverage).some((scope) => scope.usable) ? 'completed' : 'failed',
            })),
            ...failedSessions,
            ...(desktopSlices ? [{ session_id: 37, app_name: 'Interrupted Fixture', status: 'failed', ended_at: null }] : []),
            { session_id: 9, app_name: 'Another Fixture', status: 'completed' },
          ];
          case 'recent_watch_runs': return [...comparisons.values()].reverse().slice(0, 6).map(summary => ({
            app_name: comparisonNames.get(summary.session_id) ?? 'Watch Fixture', ended_at: ended, summary,
          }));
          case 'watch_comparison': {
            if (args.sessionId === 40 && !coverageFailed) {
              coverageFailed = true;
              throw new Error('coverage temporarily unavailable');
            }
            if (window.deferCoverage === args.sessionId) {
              window.deferCoverage = null;
              await new Promise(resolve => { window.releaseCoverage = resolve; });
            }
            return comparisons.get(args.sessionId) ?? null;
          }
          case 'history': {
            if (![5, 6, 100].includes(args.limit)) throw new Error('unexpected history page size');
            if (args.filter === 'Retry fixture' && !historyFailed) {
              historyFailed = true;
              throw new Error('history temporarily unavailable');
            }
            const matches = filterHistory(args);
            // Two rows per fixture page are enough to exercise both paging controls.
            const limit = args.limit === 100 ? 2 : args.limit;
            const rows = matches.slice(0, limit);
            return { rows, next_before_id: matches.length > limit ? rows.at(-1).id : null };
          }
          case 'save_changes': return { path: String.raw`C:\Users\Fixture\Documents\ExorSystem changes.json`, records: filterHistory(args).length, truncated: false };
          case 'begin_watch': {
            if (watch || capture) throw new Error('already watching');
            const { folders, registrySubtrees, system, appName } = args.request;
            const registry = registrySubtrees ? { requested: 2, usable: 1 } : { requested: 0, usable: 0 };
            const services = Number(system.services);
            const tasks = Number(system.tasks);
            const session_id = nextWatchId++;
            comparisonNames.set(session_id, appName);
            const current = {
              session_id,
              coverage: {
                folders: { requested: folders.length, usable: folders.length }, registry,
                services: { requested: services, usable: services }, tasks: { requested: tasks, usable: session_id === 40 ? 0 : tasks },
              },
              warnings: [
                ...(registrySubtrees ? ['A registry subtree was not checked before changes.'] : []),
                ...(tasks && session_id === 40 ? ['Scheduled tasks were not checked before changes: access denied.'] : []),
              ],
            };
            capture = { id: args.captureId, stopped: false };
            ready(args);
            if (window.deferNextBaseline) {
              window.deferNextBaseline = false;
              await new Promise(resolve => { window.releaseWatchCapture = resolve; });
            }
            const stopped = capture.stopped;
            capture = null;
            if (stopped) {
              failedSessions.unshift({ session_id, app_name: appName, status: 'failed', ended_at: ended });
              return null;
            }
            watch = current;
            return watch;
          }
          case 'finish_watch': {
            if (!watch || capture) throw new Error('no watch to finish');
            const current = watch;
            watch = null;
            capture = { id: args.captureId, stopped: false };
            ready(args);
            const mode = window.watchFinishMode;
            if (mode) await new Promise(resolve => { window.releaseWatchCapture = resolve; });
            else if (current.session_id === 40) await new Promise(resolve => { window.finishWatchSnapshot = resolve; });
            const stopped = capture.stopped;
            capture = null;
            if (stopped && mode !== 'committed') {
              failedSessions.unshift({ session_id: current.session_id, app_name: comparisonNames.get(current.session_id), status: 'failed', ended_at: ended });
              return null;
            }
            let summary = { ...current, ...counts };
            if (current.session_id === 43) summary = {
              ...summary, coverage: { ...current.coverage, tasks: { requested: 1, usable: 0 } },
              warnings: ['Scheduled tasks were not checked after changes: access denied.'],
            };
            else if (current.session_id === 40) summary = { ...summary, files_changed: 2, registry_keys_changed: 2 };
            else if (current.session_id !== 41 && !desktopSlices) throw new Error('file snapshot failed');
            comparisons.set(summary.session_id, summary);
            return summary;
          }
          case 'stop_watch_capture': {
            if (capture?.id !== args.captureId || !capture) return false;
            capture.stopped = true;
            return true;
          }
          case 'cancel_watch':
            if (!watch) throw new Error('no watch to cancel');
            watch = null;
            return null;
          case 'list_programs': return { programs: [
            { program: removed ? { ...app, uninstall_command: null } : app, session_ids: [], removed },
            { program: msix, session_ids: [], removed: false },
            { program: userApp, session_ids: [], removed: false },
            ...stressRows,
            ...portable.map((p) => ({program: {...standard, ...p, source: 'portable', id: 'portable:'+p.executable}, session_ids: [], removed: false})),
          ], warnings: ['One inventory source could not be checked.'] };
          case 'session_items': {
            if (args.sessionId !== 9) return { total: 0, matching: 0, items: [] };
            const total = desktopSlices ? 527 : 13;
            const matches = Array.from({ length: total }, (_, i) => ({ path: 'HKCU\\' + 'VeryLongRegistryKey'.repeat(8) + `\\Evidence-${i + 1}`,
              node_type: 'registry_key', confidence: .95, shared: i < 12 })).filter(item => item.path.toLowerCase().includes((args.filter ?? '').toLowerCase()));
            return { total, matching: matches.length, items: matches.slice(args.offset, args.offset + 12) };
          }
          case 'preview_uninstaller':
            if (args.programId !== app.id || removed) throw new Error('unsupported registration');
            return argv;
          case 'run_uninstaller':
            if (args.programId !== app.id || args.quiet || JSON.stringify(args.expected) !== JSON.stringify(argv)) throw new Error('incorrect uninstall target');
            if (invocationChanged) {
              invocationChanged = false;
              throw new Error('The registered uninstaller changed.');
            }
            if (invocationCount++ === 0) return { exit_code: 1602, summary: 'The vendor cancelled.' };
            await new Promise((resolve) => { window.finishUninstaller = resolve; });
            removed = true;
            return { exit_code: 0, summary: 'The uninstaller finished.' };
          case 'find_leftovers': return {
            folders: [
              { path: moveable, confidence: .7, reason: 'This was the install folder.', can_quarantine: true },
              { path: 'D:\\Tools\\Zed', confidence: .7, reason: 'This was the install folder.', can_quarantine: false },
            ],
            system_entries: [
              { kind: 'registry_key', path: 'HKCU\\SOFTWARE\\Zed Widget', reason: 'The key name matches the program name.' },
              { kind: 'service', path: 'ZedUpdater', reason: 'The service command points inside the install folder.' },
              { kind: 'task', path: '\\Zed\\Updater', reason: 'The task command points inside the install folder.' },
            ], warnings: ['A registry view could not be checked.'],
          };
          case 'quarantine_leftovers':
            if (args.programId !== app.id || args.folders.length !== 1 || args.folders[0] !== moveable) throw new Error('unexpected quarantine target');
            batches.unshift({ quarantine_id: 'fixture', session_id: null, status: 'quarantined', files: 2, file_bytes: 2 * 1024 ** 2 });
            return { quarantine_id: 'fixture', moved: 2, skipped: 0, summary: 'Moved 2 files to quarantine.' };
          case 'list_templates': return [{name:'standard', description:'Default'}];
          case 'list_quarantines': return batches;
          case 'quarantine_batch': {
            const batch = batches.find(b => b.quarantine_id === args.quarantineId);
            if (!batch) throw new Error('unknown batch');
            if (!previewFailed && batch.quarantine_id === 'fixture-held') { previewFailed = true; throw new Error('Recorded batch temporarily unavailable'); }
            const offset = args.offset;
            const length = Math.max(0, Math.min(24, batch.files - offset));
            const files = Array.from({ length }, (_, i) => ({ original_path: 'C:\\Fixture\\' + 'LongDestination'.repeat(9) + `\\Original-${offset + i + 1}.bin`, size: 8192 }));
            return { status: batch.status, created_unix_ms: 1794978000000, total_files: batch.files, files,
              next_offset: offset + length < batch.files ? offset + length : null };
          }
          case 'restore_quarantine': {
            const batch = batches.find(b => b.quarantine_id === args.quarantineId && b.status === 'quarantined');
            if (!batch) throw new Error('not a held batch');
            if (!restoreFailed) { restoreFailed = true; throw new Error('Original already exists; the held file was not overwritten.'); }
            batch.status = 'restored';
            return batch.files;
          }
          case 'purge_quarantine': {
            if (args.quarantineId !== 'fixture-held') throw new Error('incorrect purge target');
            const row = batches.find(b => b.quarantine_id === args.quarantineId);
            row.status = 'purged';
            return row.files;
          }
          case 'simulate': return { summary: 'Three items assessed.', keep: 1, quarantine: 1, flag_for_review: 1, decisions: [
            { path: String.raw`C:\Fixture\protected.dll`, node_type: 'file', action: 'keep', reason: 'Shared component' },
            { path: String.raw`C:\Fixture\cache.bin`, node_type: 'file', action: 'quarantine', reason: 'Recorded file' },
            { path: String.raw`HKCU\Software\Fixture`, node_type: 'registry_key', action: 'flag_for_review', reason: 'Read only' },
          ] };
          case 'app_info': return {
            version: '0.1.0', data_folder: 'profile', quarantine_folder: 'profile\\quarantine',
            legacy_data_folder: 'portable\\data', recovery_warnings: ['Batch fixture: original already exists.'],
          };
          case 'list_portable_programs': return portable;
          case 'register_portable_program':
            if (args.executable === 'does-not-exist.exe') throw new Error('enter the full local path');
            portable = [...portable, args];
            return null;
          case 'forget_portable_program': portable = portable.filter(p => p.executable !== args.executable); return null;
          case 'provenance': {
            const focus = { id: 10, kind: 'file', path: String.raw`C:\Program Files\Zed Widget\Updater.exe`, session_name: 'Zed Widget' };
            const service = { id: 11, kind: 'service', path: 'service:ZedUpdater', session_name: 'Updater service' };
            const isFile = args.path.toLowerCase() === focus.path.toLowerCase();
            if (!isFile && args.path !== service.path) throw new Error('unexpected trace path');
            return { found: true, summary: isFile ? 'The requested path matched its recorded spelling.' : 'Traced the service.', confidence: 0.7, ledger: [], graph: {
              focus: isFile ? focus : service, total_edges: 1, links: [{ node: isFile ? service : focus, edge: { edge_id: 2, parent_node_id: 10, child_node_id: 11, relationship_type: 'depends_on', confidence_score: 0.7, evidence_source: 'temporal' } }],
            } };
          }
          default: throw new Error('unhandled command '+command);
        }
      },
    };
  });
}
