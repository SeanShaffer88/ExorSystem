// Regression coverage for the next driverless desktop slices, against the built UI and mocked IPC.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { mockDesktop } from './ipc-fixture.mjs';

const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH });
try {
  const page = await browser.newPage({ viewport: { width: 1260, height: 645 } });
  page.setDefaultTimeout(12_000);
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await mockDesktop(page);
  await page.goto('https://exor.test/?desktop-slices&inventory-stress');
  const tool = (name) => page.getByRole('toolbar', { name: 'Workspace tools' }).getByRole('button', { name, exact: true });
  const button = (name) => page.getByRole('button', { name, exact: true });
  const calls = (command) => page.evaluate((name) => window.exorCalls.filter(([c]) => c === name).map(([, args]) => args), command);
  const release = async () => {
    await page.waitForFunction(() => typeof window.releaseWatchCapture === 'function');
    await page.evaluate(() => { const release = window.releaseWatchCapture; window.releaseWatchCapture = null; release(); });
  };
  const noOverflow = async () => {
    for (const panel of await page.locator('.dock-panel:visible').all()) {
      assert(await panel.evaluate((el) => el.scrollWidth <= el.clientWidth + 1), 'dock panel overflowed horizontally');
    }
  };

  const monitor = tool('Registry monitor');
  const recent = page.getByRole('region', { name: 'Recent watch comparisons' });
  await monitor.click();
  await recent.getByRole('button', { name: /Zero-change Fixture · #39 .*0 recorded changes/ }).waitFor();
  await recent.getByRole('button', { name: /Coverage-only Fixture · #38 .*Coverage only.*No scopes compared/ }).waitFor();
  assert.equal(await recent.getByText('Interrupted Fixture').count(), 0);

  // Presets persist only the scopes, not the app or installer. Applying is not an IPC capture.
  const presets = page.locator('.watch-presets');
  const app = page.getByLabel('Application name', { exact: true });
  const installer = page.getByLabel('Installer file (optional)');
  const folders = page.getByLabel('Folders to watch (optional)');
  const registry = page.getByLabel('Registry subtrees to watch (optional)');
  const services = page.getByRole('checkbox', { name: 'Services', exact: true });
  const tasks = page.getByRole('checkbox', { name: 'Scheduled tasks', exact: true });
  const folderPath = String.raw`C:\Watch Fixture`;
  const keyPath = String.raw`HKCU\Software\Watch Fixture`;
  await presets.getByText('Saved scope presets', { exact: true }).click();
  await app.fill('Private Setup Name');
  await installer.fill(String.raw`C:\Installers\PrivateSetup.exe`);
  await folders.fill(folderPath);
  await registry.fill(keyPath);
  await services.check();
  await presets.getByLabel('New preset name').fill('Fixture scopes');
  await button('Save current scopes').click();
  await presets.getByRole('status').getByText('Saved scope preset Fixture scopes.').waitFor();
  await presets.getByLabel('New preset name').fill('fixture scopes');
  await button('Save current scopes').click();
  await presets.getByRole('alert').getByText(/already uses that name/).waitFor();
  assert.deepEqual(await page.evaluate(() => JSON.parse(localStorage.getItem('exor.watch.presets.v1'))), [
    { name: 'Fixture scopes', folders: folderPath, registrySubtrees: keyPath, system: { services: true, tasks: false } },
  ]);
  await noOverflow();
  await page.reload();
  await monitor.click();
  await presets.getByText('Saved scope presets', { exact: true }).click();
  await presets.getByLabel('Scope preset', { exact: true }).selectOption('Fixture scopes');
  await button('Apply scopes').click();
  assert.equal(await app.inputValue(), '');
  assert.equal(await installer.inputValue(), '');
  assert.equal(await folders.inputValue(), folderPath);
  assert.equal(await registry.inputValue(), keyPath);
  assert(await services.isChecked());
  assert(!(await tasks.isChecked()));
  assert.equal((await calls('begin_watch')).length, 0);
  await button('Forget preset').click();
  await presets.getByRole('status').getByText('Removed Fixture scopes.').waitFor();
  assert.equal((await calls('begin_watch')).length, 0);

  // Capture IDs are specific to the in-flight read. A late request is not a cancellation result.
  await app.fill('Cancel fixture');
  await page.evaluate(() => { window.deferNextBaseline = true; });
  await button('Take baseline').click();
  const stop = button('Stop capture');
  await stop.click();
  const baseline = (await calls('begin_watch')).at(-1);
  assert.deepEqual((await calls('stop_watch_capture')).at(-1), { captureId: baseline.captureId });
  await page.getByRole('status').getByText('Stopping after the current read…').waitFor();
  await release();
  await page.getByRole('status').getByText('Baseline cancelled. No changes were recorded.').waitFor();
  assert.equal(await button('Compare snapshots').count(), 0);

  await button('Take baseline').click();
  await page.getByText(/Ready to watch Cancel fixture \(session #51\)/).waitFor();
  await page.evaluate(() => { window.watchFinishMode = 'cancel'; });
  await button('Compare snapshots').click();
  await page.waitForFunction(() => typeof window.releaseWatchCapture === 'function');
  assert.equal(await page.evaluate((captureId) => window.__TAURI_INTERNALS__.invoke('stop_watch_capture', { captureId }), baseline.captureId), false);
  await stop.click();
  const finish = (await calls('finish_watch')).at(-1);
  assert.notEqual(finish.captureId, baseline.captureId);
  assert.deepEqual((await calls('stop_watch_capture')).at(-1), { captureId: finish.captureId });
  await release();
  await page.getByRole('status').getByText(/Comparison cancelled. No changes were recorded/).waitFor();
  assert.equal(await recent.getByText(/Cancel fixture/).count(), 0);
  assert.equal((await calls('cancel_watch')).length, 0, 'in-flight stop is not the between-captures cancellation');

  await app.fill('Committed fixture');
  await button('Take baseline').click();
  await page.getByText(/Ready to watch Committed fixture \(session #52\)/).waitFor();
  await page.evaluate(() => { window.watchFinishMode = 'committed'; });
  await button('Compare snapshots').click();
  await stop.click();
  await release();
  await button('View recorded changes (session #52)').waitFor();
  await recent.getByRole('button', { name: /Committed fixture · #52 .*0 recorded changes/ }).waitFor();
  assert.equal(await page.getByText(/Comparison cancelled/).count(), 0);

  // Recent coverage-only sessions open inspector without implying zero changes. A recovered
  // session is not listed as a comparison and gets a different, truthful terminal state.
  await recent.getByRole('button', { name: /Coverage-only Fixture · #38/ }).click();
  const inspector = page.getByRole('tabpanel', { name: 'Change inspector' });
  const saved = inspector.getByRole('region', { name: 'Saved watch coverage' });
  await saved.getByText('No scopes compared', { exact: true }).waitFor();
  assert.equal(await saved.getByText(/changes recorded/).count(), 0);
  const installation = inspector.getByLabel('Installation', { exact: true });
  await installation.selectOption('37');
  await inspector.getByText('This session did not finish. No ending time was recorded.').waitFor();
  await saved.getByText(/No saved comparison for this session. Coverage cannot be inferred/).waitFor();
  await installation.selectOption('40');
  await saved.getByText('Comparison incomplete', { exact: true }).waitFor();
  const type = inspector.getByLabel('Change type', { exact: true });
  const path = inspector.getByLabel('Filter by path');
  await type.selectOption('modified');
  await path.fill('watch fixture');
  await inspector.getByRole('region', { name: 'Registry', exact: true }).getByRole('button', { name: /Changed Changed/ }).waitFor();
  await inspector.getByRole('region', { name: 'File system', exact: true }).getByRole('button', { name: /Changed Modified.txt/ }).waitFor();
  assert.equal(await inspector.getByRole('button', { name: /Removed Gone/ }).count(), 0);
  await button('Timeline').click();
  await inspector.getByRole('cell', { name: String.raw`C:\Watch Fixture\Modified.txt`, exact: true }).waitFor();
  assert.equal((await calls('history')).at(-1).filters.changeKind, 'modified');
  await inspector.getByText('Export matching changes', { exact: true }).click();
  assert.equal((await calls('save_changes')).length, 0);
  await button('Save changes JSON').click();
  await inspector.getByRole('status').getByText(/Saved 2 observation\(s\).*Documents/).waitFor();
  assert.deepEqual(await calls('save_changes'), [{ filters: { filter: 'watch fixture', sessionId: 40, changeKind: 'modified', beforeId: null, fromDate: null, beforeDate: null } }]);
  await type.selectOption('restored');
  await inspector.getByText('No recorded changes match these filters.').waitFor();
  await saved.getByText('Comparison incomplete', { exact: true }).waitFor();

  // Sorting happens before page selection. Unknown sizes are last both ways. The application
  // evidence filter searches across the entire session, including past the old 500-item bound.
  await tool('Applications').click();
  const apps = page.getByRole('region', { name: 'Application inventory' });
  const sort = apps.getByLabel('Sort applications');
  const firstApp = apps.locator('tbody tr button').first();
  await firstApp.getByText('Alpha Package', { exact: true }).waitFor();
  await sort.selectOption('largest');
  await firstApp.getByText('Zed Widget', { exact: true }).waitFor();
  await sort.selectOption('smallest');
  await firstApp.getByText('Alpha Package', { exact: true }).waitFor();
  await sort.selectOption('name');
  await page.setViewportSize({ width: 390, height: 560 });
  await button('Next').click();
  const row = apps.getByRole('button', { name: /^Inventory Fixture 24 / });
  await row.click();
  const detail = page.getByRole('complementary', { name: 'Application details' });
  await detail.getByText('527 recorded item(s)').waitFor();
  assert.equal(await detail.getByRole('button', { name: /Evidence-13 / }).count(), 0);
  await button('Next items').click();
  await detail.getByRole('button', { name: /Evidence-13 / }).waitFor();
  assert.deepEqual((await calls('session_items')).at(-1), { sessionId: 9, filter: null, offset: 12 });
  const filter = detail.getByLabel('Filter recorded items');
  await filter.fill('evidence-527');
  await detail.getByText('1 of 527 recorded items match').waitFor();
  await detail.getByRole('button', { name: /Evidence-527 / }).waitFor();
  assert.deepEqual((await calls('session_items')).at(-1), { sessionId: 9, filter: 'evidence-527', offset: 0 });
  await noOverflow();
  await button('Back to applications').click();
  assert(await row.evaluate((el) => el === document.activeElement));
  await apps.getByText(/^Page 2 of /).waitFor();

  // Preview works even for purged batches. Restore requires a readable, held record and a
  // separate confirm. Previewing, paging, changing panels, and cancelling are read-only.
  await tool('Cleanup summary').click();
  const deleted = page.getByRole('row').filter({ hasText: 'fixture-deleted' });
  const held = page.getByRole('row').filter({ hasText: 'fixture-held' });
  await deleted.getByRole('button', { name: 'View batch' }).click();
  const preview = page.getByRole('region', { name: 'Batch preview' });
  await preview.getByText('Files 1–3 of 3 recorded').waitFor();
  assert.equal(await preview.getByRole('button', { name: /restore/i }).count(), 0);
  await button('Close batch preview').click();
  const viewHeld = held.getByRole('button', { name: 'View batch' });
  await viewHeld.click();
  await preview.getByRole('alert').getByText(/temporarily unavailable/).waitFor();
  assert.equal(await button('Review restore…').count(), 0);
  await button('Retry batch preview').click();
  await preview.getByText('Files 1–24 of 27 recorded').waitFor();
  await button('Next files').click();
  await preview.getByText('Files 25–27 of 27 recorded').waitFor();
  await preview.getByText(/Original-27.bin/).waitFor();
  assert.equal((await calls('quarantine_batch')).at(-1).offset, 24);
  await button('Review restore…').click();
  const review = page.getByRole('region', { name: 'Review restore' });
  await review.getByText('Restore all 27 files to their original paths?').waitFor();
  await review.getByText(/never overwrites existing files/).waitFor();
  await noOverflow();
  assert.equal((await calls('restore_quarantine')).length, 0);
  await page.keyboard.press('Escape');
  await review.waitFor({ state: 'hidden' });
  assert(await viewHeld.evaluate((el) => el === document.activeElement));
  assert.equal((await calls('restore_quarantine')).length, 0);

  const restore = held.getByRole('button', { name: 'Restore…', exact: true });
  const confirm = button('Confirm: restore files');
  await page.setViewportSize({ width: 1260, height: 645 });
  await restore.click();
  await review.getByText('Restore all 27 files to their original paths?').waitFor();
  await confirm.click();
  await page.getByRole('alert').getByText(/Original already exists/).waitFor();
  await held.getByRole('cell', { name: 'In quarantine', exact: true }).waitFor();
  await restore.click();
  await review.getByText('Restore all 27 files to their original paths?').waitFor();
  await confirm.click();
  await page.getByRole('status').getByText('Restored 27 item(s).').waitFor();
  await held.getByRole('cell', { name: 'Restored', exact: true }).waitFor();
  assert.deepEqual(await calls('restore_quarantine'), [{ quarantineId: 'fixture-held' }, { quarantineId: 'fixture-held' }]);
  assert.deepEqual(await calls('purge_quarantine'), []);
  assert.deepEqual(errors, []);
  console.log('Desktop slices passed: scope presets, capture stops and late commits, interrupted/recent runs, filters/export, inventory and evidence paging, and explicit restore review.');
} finally { await browser.close(); }
