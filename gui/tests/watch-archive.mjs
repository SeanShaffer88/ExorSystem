// The production bundle finds profiles older than either recent list through the archive IPC.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { mockDesktop } from './ipc-fixture.mjs';

const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 800 } });
  page.setDefaultTimeout(10_000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await mockDesktop(page);
  await page.goto('https://exor.test/?watch-archive');
  const tool = name => page.getByRole('toolbar', { name: 'Workspace tools' }).getByRole('button', { name, exact: true });
  const archive = page.getByRole('tabpanel', { name: 'Watch archive' });
  const inspector = page.getByRole('tabpanel', { name: 'Change inspector' });
  const recent = page.getByRole('region', { name: 'Recent watch comparisons' });
  const saved = inspector.getByRole('region', { name: 'Saved watch coverage' });
  const coverage = name => saved.getByText(name, { exact: true });
  const results = archive.getByRole('region', { name: 'Watch archive results' });
  const row = name => results.getByRole('list').getByRole('button', { name });
  const status = text => results.getByRole('status').getByText(text, { exact: true }).waitFor();
  const browse = scope => scope.getByRole('button', { name: 'Browse Watch archive', exact: true }).click();
  const calls = command => page.evaluate(cmd => window.exorCalls.filter(([c]) => c === cmd).map(([, args]) => args), command);
  const search = async text => {
    const field = archive.getByRole('searchbox', { name: 'Application name or session ID' });
    await field.fill(text);
    await field.press('Enter');
  };
  const filter = archive.getByLabel('Session status', { exact: true });
  const older = results.getByRole('button', { name: 'Older sessions' });
  const newer = results.getByRole('button', { name: 'Newer sessions' });

  await tool('Registry monitor').click();
  await recent.getByRole('list').waitFor();
  assert.equal(await recent.getByRole('listitem').count(), 6);
  assert.equal(await recent.getByText('Archive Editor %_ Compared').count(), 0);
  assert.deepEqual(await calls('watch_archive'), [], 'an unopened archive does not query storage');
  await page.evaluate(() => { window.failArchive = true; });
  await browse(recent);
  await results.getByRole('alert').getByText(/archive temporarily unavailable/).waitFor();
  await results.getByRole('button', { name: 'Retry archive page' }).click();
  await status('Page 1 · 20 sessions');
  await row(/#192 /).waitFor();
  assert(await newer.isDisabled());
  await older.press('Enter');
  await status('Page 2 · 20 sessions');
  assert.deepEqual((await calls('watch_archive')).at(-1).filters, { search: null, status: null, beforeId: 192 });
  await row(/#191 /).waitFor();
  assert.equal(await row(/#192 /).count(), 0);
  await newer.press('Enter');
  await status('Page 1 · 20 sessions');
  assert(await newer.evaluate(el => el === document.activeElement), 'paging retains keyboard focus');
  await older.click();
  await status('Page 2 · 20 sessions');

  await filter.selectOption('completed');
  await search(' aRcHiVe EdItOr %_ ');
  await status('Page 1 · 2 sessions matching filters');
  assert.deepEqual((await calls('watch_archive')).at(-1).filters, { search: 'aRcHiVe EdItOr %_', status: 'completed', beforeId: null });
  await row(/Archive Editor %_ Legacy.*Coverage unknown/).waitFor();
  assert.equal(await row(/Archive Editor %_ Legacy.*recorded changes/).count(), 0);
  await row(/Archive Editor %_ Compared.*Comparison complete.*1 recorded change/).press('Enter');
  await coverage('Comparison complete').waitFor();
  await saved.getByText('1 change recorded', { exact: true }).waitFor();
  await inspector.getByRole('region', { name: 'File system' }).getByRole('button', { name: /Old settings.ini/ }).waitFor();
  const installation = inspector.getByLabel('Installation', { exact: true });
  assert.equal(await installation.inputValue(), '2');
  assert.equal(await installation.locator('option:checked').textContent(), 'Archive Editor %_ Compared (#2)');
  assert.equal(await installation.locator('option').count(), 152, 'the archived selection is available beyond the 150-entry picker');
  assert.equal((await calls('watch_comparison')).at(-1).sessionId, 2);
  assert.equal((await calls('history')).at(-1).filters.sessionId, 2);
  await installation.selectOption('');
  await installation.selectOption('2');
  assert.equal(await installation.inputValue(), '2');

  await browse(inspector);
  await status('Page 1 · 2 sessions matching filters');
  await search('#3');
  await results.getByText('No sessions match these filters', { exact: true }).waitFor();
  await filter.selectOption('failed');
  await search('#3');
  await row(/Archive Interrupted.*No ending time recorded.*Not completed.*No saved comparison/).click();
  await inspector.getByText('This session did not finish. No ending time was recorded.').waitFor();
  await saved.getByText(/No saved comparison for this session. Coverage cannot be inferred/).waitFor();

  await browse(inspector);
  const skipped = /Archive Coverage Only.*No scopes compared/;
  await search('#4');
  await row(skipped).waitFor();
  assert(!/recorded changes/.test(await row(skipped).textContent()));
  await row(skipped).click();
  await coverage('No scopes compared').waitFor();
  assert.equal(await saved.getByText(/changes recorded/).count(), 0);

  await browse(inspector);
  await filter.selectOption('');
  await page.evaluate(() => { window.deferArchive = '#1'; });
  await search('#1');
  await status('Loading sessions…');
  await page.waitForFunction(() => typeof window.releaseArchive === 'function');
  await search('#5');
  await row(/Archive Zero.*Comparison complete.*0 recorded changes/).waitFor();
  await page.evaluate(async () => { window.releaseArchive(); await new Promise(requestAnimationFrame); });
  await row(/Archive Zero.*0 recorded changes/).waitFor();
  assert.equal(await row(/Archive Editor %_ Legacy/).count(), 0);

  // A Watch in another dock updates the archive without refreshing or publishing a comparison.
  await archive.getByRole('button', { name: 'Clear filters' }).click();
  await status('Page 1 · 20 sessions');
  await page.getByRole('button', { name: 'Dock Watch archive', exact: true }).click();
  await page.getByRole('button', { name: 'Right dock', exact: true }).click();
  await tool('Registry monitor').click();
  const monitor = page.getByRole('tabpanel', { name: 'Registry monitor' });
  await monitor.getByLabel('Application name', { exact: true }).fill('Archive cancelled fixture');
  await monitor.getByLabel('Folders to watch (optional)').fill(String.raw`C:\Fixture`);
  await monitor.getByRole('button', { name: 'Take baseline' }).click();
  await row(/Archive cancelled fixture.*In progress.*No saved comparison/).waitFor();
  await monitor.getByRole('button', { name: 'Cancel watch' }).click();
  await row(/Archive cancelled fixture.*Not completed.*No saved comparison/).waitFor();
  assert.equal(await recent.getByText('Archive cancelled fixture').count(), 0);

  await page.setViewportSize({ width: 390, height: 560 });
  await tool('Watch archive').click();
  await status('Page 1 · 20 sessions');
  assert(await archive.evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'archive overflows a compact dock');
  assert.deepEqual(errors, []);
  console.log('Watch archive passed: bounded paging, literal/ID and status search, older inspector evidence, truthful coverage, stale responses, live cancellation, and compact layout.');
} finally {
  await browser.close();
}
