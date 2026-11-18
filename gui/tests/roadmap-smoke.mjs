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
    const msix = { ...standard, name: 'Alpha Package', source: 'msix', id: 'msix:AlphaPackage' };
    const moveable = 'C:\\Program Files\\Zed Widget';
    let portable = [];
    let removed = false;
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
    ].map(row => ({ ...row, when: '2026-11-18 05:00:00' }));
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
          ] : [];
          case 'history': {
            if (args.limit !== 100) throw new Error('unexpected history page size');
            if (args.filter === 'Retry fixture' && !historyFailed) {
              historyFailed = true;
              throw new Error('history temporarily unavailable');
            }
            const matches = historyRows.filter(row =>
              (args.sessionId === null || row.session_id === args.sessionId) &&
              (args.beforeId === null || row.id < args.beforeId) &&
              (!args.filter || row.path.toLowerCase().includes(args.filter.toLowerCase()))
            );
            // Two rows per fixture page are enough to exercise both paging controls.
            const rows = matches.slice(0, 2);
            return { rows, next_before_id: matches.length > 2 ? rows[1].id : null };
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
            ...portable.map((p) => ({program: {...standard, ...p, source: 'portable', id: 'portable:'+p.executable}, session_ids: [], removed: false})),
          ], warnings: ['One inventory source could not be checked.'] };
          case 'run_uninstaller':
            if (args.programId !== app.id) throw new Error('incorrect uninstall target');
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
            return { quarantine_id: 'fixture', moved: 2, skipped: 0, summary: 'Moved 2 files to quarantine.' };
          case 'list_templates': return [{name:'standard', description:'Default'}];
          case 'app_info': return {version:'0.1.0', data_folder:'data', quarantine_folder:'data\\quarantine'};
          case 'list_portable_programs': return portable;
          case 'register_portable_program':
            if (args.executable === 'does-not-exist.exe') throw new Error('enter the full local path');
            portable = [...portable, args];
            return null;
          case 'forget_portable_program': portable = portable.filter(p => p.executable !== args.executable); return null;
          case 'provenance':
            if (args.path !== 'c:\\program files\\ZED widget\\UPDATER.EXE') throw new Error('unexpected trace path');
            return { found: true, summary: 'The requested path matched its recorded spelling.', confidence: 0.7, origins: [], ledger: [] };
          default: throw new Error('unhandled command '+command);
        }
      },
    };
  });
  await page.goto('https://exor.test/');
  await page.getByText('One inventory source could not be checked.').waitFor();
  const actions = page.locator('.pane-actions');
  const classic = page.getByRole('option', { name: /Zed Widget/ });
  await classic.click();
  await actions.getByRole('button', {name:'Uninstall', exact:true}).waitFor();
  assert(await actions.getByRole('button', {name:'Uninstall', exact:true}).isDisabled());
  await actions.getByRole('checkbox', {name:'Run this program\'s own uninstaller'}).check();
  await actions.getByRole('button', {name:'Uninstall', exact:true}).click();
  await page.getByRole('option', {name:/Zed Widget Recently removed/}).waitFor();
  assert.equal(await actions.getByRole('button', {name:'Uninstall', exact:true}).count(), 0);
  await actions.getByRole('button', {name:'Find leftovers'}).click();
  await actions.getByRole('heading', {name:'System items (review only)'}).waitFor();
  assert.equal(await actions.getByRole('checkbox').count(), 1);
  assert(await actions.getByRole('button', {name:'Quarantine files in selected folders'}).isDisabled());
  await actions.getByText('A registry view could not be checked.').waitFor();
  await actions.getByRole('checkbox').check();
  await actions.getByRole('button', {name:'Quarantine files in selected folders'}).click();
  await actions.getByText('Moved 2 files to quarantine.').waitFor();
  await page.getByRole('option', {name:/Alpha Package/}).click();
  await actions.getByText(/Manage or remove this app in Windows Settings/).waitFor();
  assert.equal(await actions.getByRole('button', {name:'Find leftovers'}).count(), 0);
  assert.equal(await actions.getByRole('button', {name:'Uninstall', exact:true}).count(), 0);
  assert.equal(await actions.getByRole('heading', {name:'System items (review only)'}).count(), 0);
  await page.getByRole('tab', {name:'Settings'}).click();
  await page.getByRole('heading', {name:'Portable apps'}).waitFor();
  await page.getByLabel('Program name').fill('Portable Fixture');
  await page.getByLabel('Full path to the .exe file').fill('does-not-exist.exe');
  await page.getByRole('button', {name:'Save portable app'}).click();
  await page.getByRole('alert').filter({hasText: 'enter the full local path'}).waitFor();
  await page.getByLabel('Full path to the .exe file').fill('C:\\Tools\\Fixture.exe');
  await page.getByRole('button', {name:'Save portable app'}).click();
  await page.getByRole('button', {name:'Forget Portable Fixture'}).waitFor();
  await page.getByRole('tab', {name:'Installations'}).click();
  await page.getByRole('option', {name:/Portable Fixture Portable app/}).click();
  await actions.getByText(/Manage this inventory entry/).waitFor();
  assert.equal(await actions.getByRole('button', {name:'Uninstall', exact:true}).count(), 0);
  await page.getByRole('tab', {name:'Settings'}).click();
  await page.getByRole('button', {name:'Forget Portable Fixture'}).click();
  await page.getByText('No portable apps added.').waitFor();
  await page.getByRole('tab', {name:'Provenance'}).click();
  await page.getByLabel('File path or registry key').fill('c:\\program files\\ZED widget\\UPDATER.EXE');
  await page.getByRole('button', {name:'Trace'}).click();
  await page.getByText('The requested path matched its recorded spelling.').waitFor();
  const watchTab = page.getByRole('tab', {name:'Watch an install'});
  const provenanceTab = page.getByRole('tab', {name:'Provenance'});
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
  await page.getByRole('heading', {name: 'History', exact: true}).waitFor();
  const historyTab = page.getByRole('tab', {name: 'History', exact: true});
  assert(await historyTab.evaluate(tab => document.activeElement === tab));
  const historySession = page.getByLabel('Installation', {exact: true});
  await historySession.getByRole('option', {name: 'Watch Fixture (#40)', exact: true}).waitFor({state: 'attached'});
  assert.equal(await historySession.inputValue(), '40');
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
  assert.equal(calls.filter(([c]) => c === 'run_uninstaller').length, 1);
  assert.equal(calls.filter(([c]) => c === 'quarantine_leftovers').length, 1);
  assert.deepEqual(calls.filter(([c]) => c === 'begin_watch').map(([, args]) => args.registrySubtrees), [subtrees, '', '']);
  assert.equal(calls.filter(([c]) => c === 'finish_watch').length, 2);
  assert.equal(calls.filter(([c]) => c === 'cancel_watch').length, 1);
  const historyCalls = calls.filter(([c]) => c === 'history').map(([, args]) => args);
  assert.deepEqual(historyCalls[0], { filter: null, limit: 100, sessionId: 40, beforeId: null });
  assert(historyCalls.some(args => args.sessionId === 40 && args.beforeId === 41));
  assert(historyCalls.some(args => args.sessionId === 40 && args.filter === 'gone' && args.beforeId === null));
  console.log('Browser smoke passed: inventory, uninstall/review, quarantine, portable inventory, provenance, registry watch, and session history');
} finally {
  await browser.close();
}
