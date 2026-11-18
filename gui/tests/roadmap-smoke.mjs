// Run against the production bundle; stub only the native Tauri IPC boundary.
import { chromium } from 'playwright-core';
import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import path from 'node:path';

const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH });
try {
  const page = await browser.newPage({ viewport: { width: 1100, height: 720 } });
  const errors = [];
  page.on('pageerror', err => errors.push(err.message));
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
    const app = { ...standard, name: 'Zed Widget', source: 'registry', id: 'HKLM\\Zed', scope: 'machine', uninstall_command: 'C:\\Program Files\\Zed Widget\\uninstall.exe' };
    const userApp = { ...app, id: 'HKCU\\Fixture', name: 'Per-user Fixture', scope: 'user' };
    const msix = { ...standard, name: 'Alpha Package', source: 'msix', id: 'msix:AlphaPackage' };
    const moveable = 'C:\\Program Files\\Zed Widget';
    let portable = [];
    let removed = false;
    let invocationChanged = true;
    let invocationCount = 0;
    const argv = { executable: `${moveable}\\uninstall.exe`, args: ['--interactive', 'two words'] };
    const batches = [
      { quarantine_id: 'fixture-held', session_id: null, status: 'quarantined', files: 2, file_bytes: 4 * 1024 ** 2 },
      { quarantine_id: 'fixture-deleted', session_id: 9, status: 'purged', files: 3, file_bytes: 100 * 1024 ** 2 },
    ];
    let watch = null;
    let nextWatchId = 40;
    let watchRecorded = false;
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
    window.exorCalls = calls;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        calls.push([command, args]);
        switch (command) {
          case 'plugin:event|listen': return 1;
          case 'plugin:event|unlisten': return null;
          case 'get_log': return [];
          case 'log_is_open': return false;
          case 'list_sessions': return watchRecorded ? [
            { session_id: 40, app_name: 'Watch Fixture', status: 'completed' },
            { session_id: 9, app_name: 'Another Fixture', status: 'completed' },
          ] : [{ session_id: 9, app_name: 'Another Fixture', status: 'completed' }];
          case 'history': {
            if (![5, 6, 100].includes(args.limit)) throw new Error('unexpected history page size');
            if (args.filter === 'Retry fixture' && !historyFailed) {
              historyFailed = true;
              throw new Error('history temporarily unavailable');
            }
            const matches = historyRows.filter(row =>
              (args.domain === null || row.domain === args.domain) &&
              (args.sessionId === null || row.session_id === args.sessionId) &&
              (args.beforeId === null || row.id < args.beforeId) &&
              (!args.filter || row.path.toLowerCase().includes(args.filter.toLowerCase()))
            );
            // Two rows per fixture page are enough to exercise both paging controls.
            const limit = args.limit === 100 ? 2 : args.limit;
            const rows = matches.slice(0, limit);
            return { rows, next_before_id: matches.length > limit ? rows.at(-1).id : null };
          }
          case 'begin_watch':
            if (watch) throw new Error('already watching');
            watch = {
              session_id: nextWatchId++,
              warnings: args.registrySubtrees ? ['A registry subtree was not checked before the install.'] : [],
            };
            return watch;
          case 'finish_watch': {
            if (!watch) throw new Error('no watch to finish');
            const current = watch;
            watch = null;
            if (current.session_id !== 40) throw new Error('file snapshot failed');
            await new Promise(resolve => { window.finishWatchSnapshot = resolve; });
            watchRecorded = true;
            return {
              ...current, files_changed: 1, services_changed: 0, tasks_changed: 0,
              registry_keys_changed: 2, registry_scopes_compared: 1,
              summary: 'Recorded 1 file change for Watch Fixture.',
            };
          }
          case 'cancel_watch':
            if (!watch) throw new Error('no watch to cancel');
            watch = null;
            return null;
          case 'list_programs': return { programs: [
            { program: removed ? { ...app, uninstall_command: null } : app, session_ids: [], removed },
            { program: msix, session_ids: [], removed: false },
            { program: userApp, session_ids: [], removed: false },
            ...portable.map((p) => ({program: {...standard, ...p, source: 'portable', id: 'portable:'+p.executable}, session_ids: [], removed: false})),
          ], warnings: ['One inventory source could not be checked.'] };
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
  const tab = (name) => page.getByRole('tab', { name, exact: true });
  const choose = (name) => page.getByRole('region', { name: 'Application inventory' }).getByRole('button', { name }).click();
  const button = (name) => page.getByRole('button', { name, exact: true });
  const callsOf = (name) => page.evaluate((c) => window.exorCalls.filter(([command]) => command === c), name);

  await page.goto('https://exor.test/');
  await page.getByRole('heading', { name: 'System overview' }).waitFor();
  assert.equal(await page.getByText('Demo data').count(), 0);
  await page.getByRole('region', { name: 'Storage reclamation' }).getByText('4 MiB', { exact: true }).waitFor();
  await page.getByRole('article').filter({ hasText: 'Deleted in cleanup' }).getByText('100 MiB', { exact: true }).waitFor();
  await button('View inventory').click();
  await page.getByText('One inventory source could not be checked.').waitFor();
  const details = page.getByRole('complementary', { name: 'Application details' });
  await choose(/Per-user Fixture/);
  await details.getByText(/Use Windows Settings → Apps to uninstall this per-user program/).waitFor();
  assert.equal(await details.getByRole('button', {name:'Uninstall wizard'}).count(), 0);
  await details.getByRole('button', {name:'Review leftovers'}).click();
  await page.getByRole('dialog').getByText(/This wizard has not run a vendor uninstaller/).waitFor();
  await page.keyboard.press('Escape');

  await choose(/Zed Widget/);
  await details.getByRole('button', { name: 'Preview command' }).click();
  const terminal = page.getByRole('region', { name: 'Read-only execution preview' });
  await terminal.getByText('"two words"', { exact: true }).waitFor();
  assert.equal(await terminal.locator('input, textarea, [contenteditable]').count(), 0);
  assert.equal((await callsOf('run_uninstaller')).length, 0);
  await tab('Applications').click();
  await choose(/Zed Widget/);

  const wizard = page.getByRole('dialog');
  const advanceToLaunch = async () => {
    await details.getByRole('button', { name: 'Uninstall wizard' }).click();
    await wizard.getByText('Step 1 of 4').waitFor();
    await wizard.getByRole('button', { name: 'Continue', exact: true }).click();
    await wizard.getByText('Step 2 of 4').waitFor();
    assert(await wizard.getByRole('button', { name: 'Run uninstaller' }).isDisabled());
    await wizard.getByRole('checkbox', { name: 'Run this program’s own uninstaller' }).check();
    await wizard.getByRole('button', { name: 'Run uninstaller' }).click();
  };
  await advanceToLaunch();
  await wizard.getByRole('alert').getByText('The registered uninstaller changed.').waitFor();
  assert(await wizard.getByRole('button', { name: 'Run uninstaller' }).isDisabled());
  await wizard.getByRole('button', { name: 'Close wizard' }).click();
  await advanceToLaunch();
  await wizard.getByText('Uninstall not confirmed').waitFor();
  await wizard.getByText(/Exit code: 1602/).waitFor();
  await wizard.getByRole('button', { name: 'Close wizard' }).click();
  await advanceToLaunch();
  await wizard.getByRole('progressbar', { name: 'Vendor uninstaller running' }).waitFor();
  assert(await wizard.getByRole('button', { name: 'Close wizard' }).isDisabled());
  await page.keyboard.press('Escape');
  assert(await wizard.isVisible());
  await page.evaluate(() => window.finishUninstaller());
  await wizard.getByText('Vendor process exited').waitFor();
  assert.equal((await callsOf('find_leftovers')).length, 0);
  await wizard.getByRole('button', {name:'Continue to leftover review'}).click();
  await wizard.getByRole('button', {name:'Find leftovers'}).click();
  await wizard.getByRole('heading', {name:'System items (review only)'}).waitFor();
  assert.equal(await wizard.getByRole('checkbox').count(), 1);
  const quarantine = wizard.getByRole('button', {name:'Quarantine selected folders…'});
  assert(await quarantine.isDisabled());
  await wizard.getByText('A registry view could not be checked.').waitFor();
  await wizard.getByRole('checkbox').check();
  await quarantine.click();
  assert.equal((await callsOf('quarantine_leftovers')).length, 0);
  assert(await wizard.getByRole('checkbox').isDisabled());
  await wizard.getByRole('button', {name:'Confirm: move files'}).click();
  await wizard.getByText('Step 4 of 4').waitFor();
  await wizard.getByText('Moved 2 files to quarantine.', { exact: false }).waitFor();
  await wizard.getByRole('button', {name:'Done'}).click();
  await page.getByRole('button', {name:/Zed Widget Recently removed/}).waitFor();
  assert.equal(await details.getByRole('button', {name:'Uninstall wizard'}).count(), 0);
  await choose(/Alpha Package/);
  await details.getByText(/Manage or remove this app in Windows Settings/).waitFor();
  assert.equal(await details.getByRole('button', {name:'Review leftovers'}).count(), 0);
  assert.equal(await details.getByRole('button', {name:'Uninstall wizard'}).count(), 0);

  await tab('Cleanup summary').click();
  await page.getByRole('alert').getByText('Batch fixture: original already exists.').waitFor();
  const held = page.getByRole('row').filter({ hasText: 'fixture-held' });
  await held.getByRole('button', { name: 'Delete permanently…' }).click();
  assert.equal((await callsOf('purge_quarantine')).length, 0);
  await held.getByRole('button', { name: 'Confirm: delete' }).click();
  await page.getByRole('region', {name:'Storage reclamation'}).getByText('104 MiB', { exact: true }).first().waitFor();
  await held.getByRole('cell', {name:'Deleted permanently'}).waitFor();
  await tab('Policy simulator').click();
  await page.getByLabel('Installation', {exact: true}).selectOption('9');
  await button('Run dry run').click();
  await page.getByRole('cell', {name:'Protected · keep'}).waitFor();
  await page.getByRole('cell', {name:'Quarantine candidate'}).waitFor();
  await page.getByRole('cell', {name:'Review only'}).waitFor();

  await tab('Settings').click();
  await page.getByText(/Older portable data is unchanged/).waitFor();
  await page.getByLabel('Program name').fill('Portable Fixture');
  await page.getByLabel('Full path to the .exe file').fill('does-not-exist.exe');
  await button('Save portable app').click();
  await page.getByRole('alert').filter({hasText: 'enter the full local path'}).waitFor();
  await page.getByLabel('Full path to the .exe file').fill(String.raw`C:\Tools\Fixture.exe`);
  await button('Save portable app').click();
  await button('Forget Portable Fixture').waitFor();
  await tab('Applications').click();
  await page.getByLabel('Application source').selectOption('portable');
  await choose(/Portable Fixture/);
  await details.getByText(/Manage this inventory entry/).waitFor();
  assert.equal(await details.getByRole('button', {name:'Uninstall wizard'}).count(), 0);
  await tab('Settings').click();
  await button('Forget Portable Fixture').click();
  await page.getByText('No portable apps added.').waitFor();

  await tab('Relationships').click();
  await page.getByLabel('File path or registry key').fill(String.raw`c:\program files\ZED widget\UPDATER.EXE`);
  await button('Trace').click();
  await page.getByText('The requested path matched its recorded spelling.').waitFor();
  const graph = page.getByRole('region', { name: 'Relationship graph' });
  await graph.getByRole('button', { name: /Depends on focus.*Updater service.*70%/ }).click();
  await graph.getByRole('button', { name: /Focus depends on.*Zed Widget.*70%/ }).waitFor();
  const watchTab = tab('Registry monitor');
  const provenanceTab = tab('Relationships');
  await watchTab.click();
  const begin = page.getByRole('button', {name:'Take the first picture', exact:true});
  const finish = page.getByRole('button', {name:'Finish: take the second picture'});
  const registry = page.getByLabel('Registry subtrees to watch (optional)');
  const subtrees = String.raw`HKCU\Software\Watch Fixture` + '\n' + String.raw`HKLM32\Software\Watch Fixture`;
  await page.getByLabel('Application name', {exact:true}).fill('Watch Fixture');
  await registry.fill(subtrees);
  await begin.click();
  await page.getByText(/Ready to watch Watch Fixture/).waitFor();
  await page.getByText('A registry subtree was not checked before the install.').waitFor();
  await provenanceTab.click();
  assert(!(await finish.isVisible()));
  await watchTab.click();
  assert(await finish.isEnabled());
  await finish.click();
  await page.getByText('Taking the after picture…').waitFor();
  await provenanceTab.click();
  await page.evaluate(() => window.finishWatchSnapshot());
  await watchTab.click();
  await page.getByText(/Registry key changes: 2 across 1/).waitFor();
  await page.getByText('A registry subtree was not checked before the install.').waitFor();
  assert.equal(await registry.inputValue(), subtrees);

  // Open the recorded evidence for the completed watch, not the newest global changes.
  await page.getByRole('button', {name: 'View recorded changes (session #40)', exact: true}).click();
  await page.getByRole('heading', {name: 'Change inspector', exact: true}).waitFor();
  const historyTab = tab('Change inspector');
  assert(await historyTab.evaluate(tab => document.activeElement === tab));
  const historySession = page.getByLabel('Installation', {exact: true});
  await historySession.getByRole('option', {name: 'Watch Fixture (#40)', exact: true}).waitFor({state: 'attached'});
  assert.equal(await historySession.inputValue(), '40');
  const registryPane = page.getByRole('region', { name: 'Registry', exact: true });
  const filePane = page.getByRole('region', { name: 'File system', exact: true });
  const diff = page.getByRole('region', { name: 'Before and after evidence', exact: true });
  await registryPane.getByRole('button', { name: /Removed Gone/ }).click();
  await diff.getByText('Not present in follow-up').waitFor();
  await filePane.getByRole('button', { name: /Changed Modified.txt/ }).click();
  await diff.getByText('a'.repeat(64), { exact: true }).waitFor();
  await diff.getByText('b'.repeat(64), { exact: true }).waitFor();
  await button('Timeline').click();
  const removedKey = page.getByRole('row').filter({hasText: String.raw`HKCU\Software\Watch Fixture\Gone`});
  await removedKey.getByRole('cell', {name: 'Removed', exact: true}).waitFor();
  await removedKey.getByRole('cell', {name: 'Before/after snapshots', exact: true}).waitFor();
  assert.equal(await page.getByText('Other session.dat', {exact: false}).count(), 0);
  const olderChanges = page.getByRole('button', {name: 'Older changes'});
  const newerChanges = page.getByRole('button', {name: 'Newer changes'});
  const olderItem = page.getByRole('cell', {name: String.raw`C:\Watch Fixture\Older.txt`, exact: true});
  await olderChanges.click();
  await olderItem.waitFor();
  assert(await olderChanges.isDisabled());
  await newerChanges.click();
  await removedKey.waitFor();
  assert(await newerChanges.isDisabled());
  await olderChanges.click();
  await olderItem.waitFor();
  const pathFilter = page.getByLabel('Filter by path');
  await pathFilter.fill('gone');
  await removedKey.waitFor();
  assert(await newerChanges.isDisabled());
  assert(await olderChanges.isDisabled());
  await pathFilter.fill('Retry fixture');
  await page.getByRole('alert').filter({hasText: 'history temporarily unavailable'}).waitFor();
  await page.getByRole('button', {name: 'Retry', exact: true}).click();
  await page.getByText('No recorded changes match these filters.').waitFor();
  assert.equal(await page.getByRole('alert').count(), 0);
  await pathFilter.fill('');
  await historySession.selectOption('');
  await page.getByRole('cell', {name: 'Unassigned', exact: true}).waitFor();
  await page.getByRole('cell', {name: String.raw`C:\Other session.dat`, exact: true}).waitFor();
  await watchTab.click();

  // A failed Finish consumes the watch. Offer a fresh baseline, not a dead Finish button.
  await registry.fill('');
  await begin.click();
  await finish.click();
  await page.getByRole('alert').filter({hasText: 'Start a new watch to try again.'}).waitFor();
  assert(await begin.isEnabled());
  assert.equal(await finish.count(), 0);

  await begin.click();
  await finish.waitFor();
  await provenanceTab.click();
  await watchTab.click();
  await page.getByRole('button', {name:'Cancel', exact:true}).click();
  await begin.waitFor();
  assert.deepEqual(errors, []);
  const calls = await page.evaluate(() => window.exorCalls);
  assert.equal(calls.filter(([c]) => c === 'run_uninstaller').length, 3);
  assert.equal(calls.filter(([c]) => c === 'purge_quarantine').length, 1);
  assert.equal(calls.filter(([c]) => c === 'quarantine_leftovers').length, 1);
  assert.deepEqual(calls.filter(([c]) => c === 'begin_watch').map(([, args]) => args.registrySubtrees), [subtrees, '', '']);
  assert.equal(calls.filter(([c]) => c === 'finish_watch').length, 2);
  assert.equal(calls.filter(([c]) => c === 'cancel_watch').length, 1);
  const historyCalls = calls.filter(([c]) => c === 'history').map(([, args]) => args);
  assert(historyCalls.some(args => args.sessionId === 40 && args.domain === 'registry' && args.limit === 6));
  assert(historyCalls.some(args => args.sessionId === 40 && args.domain === 'file' && args.limit === 6));
  assert(historyCalls.some(args => args.sessionId === 40 && args.domain === null && args.limit === 100));
  assert(historyCalls.some(args => args.sessionId === 40 && args.beforeId === 41));
  assert(historyCalls.some(args => args.sessionId === 40 && args.filter === 'gone' && args.beforeId === null));
  await page.setViewportSize({width: 390, height: 844});
  await tab('Overview').click();
  await page.getByRole('region', { name: 'Installed applications' }).getByRole('button', { name: /Alpha Package/ }).waitFor();
  assert(await page.locator('#main').evaluate((el) => el.scrollWidth <= el.clientWidth));
  await page.keyboard.press('ArrowDown');
  assert.equal(await tab('Applications').getAttribute('aria-selected'), 'true');
  console.log('Browser smoke passed: dashboard metrics, read-only preview, confirmed wizard, cleanup totals, inventory, graph traversal, registry watch, split diff, and history pagination.');
} finally {
  await browser.close();
}
