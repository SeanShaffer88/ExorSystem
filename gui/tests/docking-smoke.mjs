import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { mockDesktop } from './ipc-fixture.mjs';

const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH });
const layoutKey = 'exor.workspace.layout.v1';
try {
  const page = await browser.newPage({ viewport: { width: 1400, height: 840 } });
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await mockDesktop(page);
  const btn = name => page.getByRole('button', { name, exact: true });
  const tab = name => page.getByRole('tab', { name, exact: true });
  const panel = name => page.getByRole('tabpanel', { name, exact: true });
  const list = name => page.getByRole('tablist', { name });
  const tool = name => page.getByRole('toolbar', { name: 'Workspace tools' }).getByRole('button', { name, exact: true });
  const focused = el => el.evaluate(e => e === document.activeElement);
  const mainTabs = list('Main panels');
  const rightTabs = list('Right dock panels');
  const bottomTabs = list('Bottom dock panels');
  const right = page.getByRole('separator', { name: 'Resize right dock' });
  const bottom = page.getByRole('separator', { name: 'Resize bottom dock' });
  const callsOf = command => page.evaluate(cmd => window.exorCalls.filter(([name]) => name === cmd), command);
  const saved = () => page.evaluate(key => JSON.parse(localStorage.getItem(key)), layoutKey);
  const preset = async (name) => {
    await btn('Workspace layout').click();
    await page.getByRole('button', { name }).click();
  };
  const keyboardMove = async (name, area) => {
    const toggle = btn(`Dock ${name}`);
    await toggle.focus();
    await page.keyboard.press('Enter');
    assert.equal(await toggle.getAttribute('aria-expanded'), 'true');
    // Move options are ordinary tab stops, not reliant on a cursor or drag gesture.
    await page.keyboard.press('Tab');
    if (!await focused(btn(area))) await page.keyboard.press('Tab');
    assert(await focused(btn(area)));
    await page.keyboard.press('Enter');
    assert(await focused(tab(name)));
  };
  const drag = async (source, target) => {
    await source.hover();
    const rect = await source.boundingBox();
    const x = rect.x + rect.width / 2, y = rect.y + rect.height / 2;
    await page.mouse.down();
    await page.mouse.move(x + 12, y, { steps: 4 });
    await target.waitFor();
    await target.hover();
    await target.hover(); // Chromium can require a second move over a new drop target.
    await page.mouse.up();
  };

  await page.goto('https://exor.test/?deferred-log');
  await panel('Overview').waitFor();
  await page.keyboard.press('Tab');
  assert(await focused(page.getByRole('link', { name: 'Skip to workspace' })));
  await page.keyboard.press('Enter');
  assert(await focused(panel('Overview')));

  await tab('Overview').press('End');
  assert(await focused(tab('Applications')));
  await tab('Applications').press('Control+ArrowLeft');
  assert.deepEqual(await mainTabs.getByRole('tab').allTextContents(), ['Applications', 'Overview']);
  await tab('Applications').press('Delete');
  assert(await focused(tab('Overview')));
  await tool('Overview').focus();
  await page.keyboard.press('ArrowDown');
  assert(await focused(tool('Applications')));
  assert.equal(await tool('Applications').getAttribute('aria-current'), 'page');
  assert.deepEqual(await mainTabs.getByRole('tab').allTextContents(), ['Overview', 'Applications']);

  await btn('Workspace layout').press('Enter');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Escape');
  assert(await focused(btn('Workspace layout')));
  assert.equal(await btn('Workspace layout').getAttribute('aria-expanded'), 'false');
  await drag(tab('Applications'), page.locator('.dock-drop-target[data-area="right"]'));
  await rightTabs.getByRole('tab', { name: 'Applications', exact: true }).waitFor();
  assert(await focused(tab('Applications')));
  assert(await panel('Overview').isVisible());

  await tool('Script preview').click();
  await keyboardMove('Script preview', 'Bottom dock');
  await bottomTabs.getByRole('tab', { name: 'Script preview' }).waitFor();
  const rightWidth = (await panel('Applications').boundingBox()).width;
  await right.press('ArrowLeft');
  assert.equal(await right.getAttribute('aria-valuenow'), '42');
  assert((await panel('Applications').boundingBox()).width > rightWidth + 15);
  await right.press('Home');
  assert.equal(await right.getAttribute('aria-valuenow'), '34');
  await bottom.press('End');
  assert.equal(await bottom.getAttribute('aria-valuenow'), '52');
  const splitter = await bottom.boundingBox();
  const x = splitter.x + splitter.width / 2, y = splitter.y + splitter.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y + 83, { steps: 6 });
  await page.mouse.up();
  const bottomValue = Number(await bottom.getAttribute('aria-valuenow'));
  assert(bottomValue >= 38 && bottomValue <= 43, `unexpected bottom split: ${bottomValue}`);

  const overview = panel('Overview');
  const applications = panel('Applications');
  await overview.getByRole('region', { name: 'Installed applications' }).getByRole('button', { name: /Zed Widget/ }).click();
  assert(await focused(tab('Applications')));
  await applications.getByRole('complementary', { name: 'Application details' }).getByRole('button', { name: 'Preview command' }).click();
  await panel('Script preview').getByText('"two words"', { exact: true }).waitFor();
  assert(await focused(tab('Script preview')));
  assert(await applications.isVisible());
  assert.equal(await rightTabs.getByRole('tab').count(), 1);
  await applications.getByRole('button', { name: 'Uninstall wizard', exact: true }).click();
  const wizard = page.getByRole('dialog');
  await wizard.getByText('Step 1 of 4').waitFor();
  await page.keyboard.press('Alt+4');
  await page.keyboard.press('Control+Backquote');
  assert(await wizard.isVisible());
  assert.equal(await page.locator('#tab-activity').count(), 0);
  assert.equal(await page.locator('#tab-history').count(), 0);
  await page.keyboard.press('Escape');

  // Preserve a second dock's selected tool while the activity stream comes forward.
  await page.keyboard.press('Control+Backquote');
  const activity = panel('Activity log');
  const log = activity.getByRole('log', { name: 'Activity messages' });
  await log.waitFor();
  assert(await focused(tab('Activity log')));
  await page.waitForFunction(() => !!window.releaseExorLog);
  await page.evaluate(() => {
    const line = { seq: 2, t: 1, level: 'warn', msg: 'A warning' };
    window.appendExorLog({ seq: 3, t: 1, level: 'error', msg: 'A failure' });
    window.appendExorLog(line);
    window.emitExor('log', line);
  });
  await log.getByText(/A failure/).waitFor();
  assert.deepEqual(await log.locator('.log-l').allTextContents(), ['WARN', 'FAIL']);
  assert.equal(await log.getByText(/A warning/).count(), 1);
  assert.equal(await page.locator('.status-text').textContent(), 'A failure');
  await activity.getByRole('button', { name: 'Clear log', exact: true }).click();
  await log.getByText('No activity yet.').waitFor();
  await page.evaluate(() => {
    window.releaseExorLog(); // Response from before the clear; must not bring the old line back.
    window.emitExor('log', { seq: 2, t: 1, level: 'warn', msg: 'Late, already cleared' });
    window.appendExorLog({ seq: 4, t: 1, level: 'info', msg: 'After clear' });
    window.emitExor('log-cleared', 3);
  });
  await log.getByText(/After clear/).waitFor();
  assert.equal(await log.locator('.logline').count(), 1);
  await btn('Open attached window').click();
  await btn('Hide attached window').waitFor();
  await btn('Hide attached window').click();
  assert.deepEqual((await callsOf('set_log_open')).map(([, args]) => args.open), [true, false]);

  await preset(/Investigation layout/);
  assert(await focused(tab('Change inspector')));
  await panel('Change inspector').getByLabel('Filter by path').fill('No matching path');
  const watch = panel('Registry monitor');
  await watch.getByLabel('Application name', { exact: true }).fill('Watch Fixture');
  await watch.getByLabel('Folders to watch (optional)').fill(String.raw`C:\Watch Fixture`);
  await watch.getByRole('button', { name: 'Take baseline', exact: true }).click();
  await watch.getByText(/Ready to watch Watch Fixture/).waitFor();
  await tab('Registry monitor').press('Delete');
  assert(await focused(tab('Relationships')));
  assert.equal(await watch.count(), 0);
  assert.match(await tool('Registry monitor').getAttribute('aria-description'), /Watch in progress/);
  await tool('Registry monitor').click();
  await rightTabs.getByRole('tab', { name: 'Registry monitor' }).waitFor();
  await watch.getByRole('button', {name: 'Compare snapshots', exact: true}).click();
  await watch.getByText('Comparing snapshots…', { exact: true }).waitFor();
  await keyboardMove('Registry monitor', 'Bottom dock');
  await tab('Activity log').click();
  assert.equal(await watch.count(), 0);
  await page.evaluate(() => window.finishWatchSnapshot());
  await tool('Registry monitor').click();
  await watch.getByRole('button', { name: 'View recorded changes (session #40)', exact: true }).click();
  assert(await focused(tab('Change inspector')));
  const inspect = panel('Change inspector');
  assert.equal(await inspect.getByLabel('Installation', { exact: true }).inputValue(), '40');
  assert.equal(await inspect.getByLabel('Filter by path').inputValue(), '');
  assert.equal((await callsOf('begin_watch')).length, 1);
  assert.equal((await callsOf('finish_watch')).length, 1);
  assert.equal((await callsOf('cancel_watch')).length, 0);

  await tool('Scan folder').click();
  const scan = panel('Scan folder');
  await scan.getByLabel('Folder', { exact: true }).fill(String.raw`C:\Scan Fixture`);
  await scan.getByRole('button', { name: 'Scan', exact: true }).click();
  await scan.getByRole('progressbar', { name: 'Scan progress' }).waitFor();
  await keyboardMove('Scan folder', 'Right dock');
  assert.equal(await scan.getByLabel('Folder', { exact: true }).inputValue(), String.raw`C:\Scan Fixture`);
  await preset(/Cleanup layout/);
  assert.equal(await scan.count(), 0);
  await page.evaluate(() => window.finishExorScan());
  await tool('Scan folder').click();
  await scan.getByRole('status').getByText(/Scan finished. 6 items recorded/).waitFor();
  assert.equal((await callsOf('scan')).length, 1);

  await tool('Appearance').click();
  await panel('Appearance').getByRole('radio', { name: /Minimalist/ }).check();
  assert.equal(await page.locator('html').getAttribute('data-theme'), 'minimalist');
  await tool('Cleanup summary').click();
  assert.equal(await page.locator('html').getAttribute('data-theme'), 'default');
  const destructive = /^(?:run_uninstaller|quarantine_.*|restore_quarantine|purge_quarantine)$/;
  assert(!(await page.evaluate(() => window.exorCalls)).some(([name]) => destructive.test(name)));

  // Persist placement, order, focus and percentages across an actual reload of the app bundle.
  await right.press('End');
  await bottom.press('ArrowUp');
  await keyboardMove('Cleanup summary', 'Right dock');
  await tab('Activity log').press('Delete');
  const before = await saved();
  await page.goto('https://exor.test/');
  await rightTabs.getByRole('tab', { name: 'Cleanup summary' }).waitFor();
  assert.deepEqual(await saved(), before);
  assert.equal(await right.getAttribute('aria-valuenow'), String(before.sizes.right));
  await btn('Toggle activity log panel').click();
  await bottomTabs.getByRole('tab', { name: 'Activity log' }).waitFor();
  const wide = await saved();
  const switcher = page.getByRole('navigation', { name: 'Compact docks' });
  for (const size of [{ width: 800, height: 560 }, { width: 390, height: 844 }, { width: 1360, height: 410 }]) {
    await page.setViewportSize(size);
    await switcher.waitFor();
    assert.equal(await page.getByRole('separator').count(), 0);
    await switcher.getByRole('button', { name: 'Right dock', exact: true }).click();
    await panel('Cleanup summary').waitFor();
    assert.equal(await page.getByRole('tabpanel').count(), 1);
    assert(await panel('Cleanup summary').evaluate(e => e.scrollWidth <= e.clientWidth));
    assert.deepEqual((await saved()).sizes, wide.sizes);
  }
  await page.setViewportSize({ width: 1400, height: 840 });
  await right.waitFor();
  assert.deepEqual((await saved()).groups, wide.groups);
  assert.equal(await page.getByRole('tabpanel').count(), 3);
  await page.emulateMedia({ forcedColors: 'active', reducedMotion: 'reduce' });
  await tab('Cleanup summary').press('Home');
  assert(await focused(rightTabs.getByRole('tab', { name: 'Policy simulator' })));

  // Validate malformed, outdated and duplicate panel preferences, then clear back to defaults.
  const inject = (value) => page.evaluate(({ key, value }) => localStorage.setItem(key, value), { key: layoutKey, value });
  await inject(JSON.stringify({ version: 1, groups: {
    main: { ids: ['overview', 'overview', 'not-a-tool', 7], active: 'unavailable' },
    right: { ids: ['overview', 'history'], active: 'overview' },
    bottom: { ids: 'malformed' },
  }, sizes: { right: -40, bottom: 999 }, focus: 'missing', reopen: { activity: 'nowhere', scan: 'right' } }));
  await page.reload();
  await rightTabs.getByRole('tab', { name: 'Change inspector' }).waitFor();
  const fixed = await saved();
  assert.deepEqual(fixed.groups.main, { ids: ['overview'], active: 'overview' });
  assert.deepEqual(fixed.groups.right, { ids: ['history'], active: 'history' });
  assert.deepEqual(fixed.groups.bottom, { ids: [], active: null });
  assert.equal(fixed.focus, 'main');
  assert.deepEqual(fixed.sizes, { right: 34, bottom: 52 });
  assert.deepEqual(fixed.reopen, { scan: 'right' });
  await preset(/Reset layout/);
  assert.deepEqual(await mainTabs.getByRole('tab').allTextContents(), ['Overview', 'Applications']);
  assert.equal(await right.count(), 0);
  await tab('Overview').press('Delete');
  await tab('Applications').press('Delete');
  await page.getByRole('heading', { name: 'Main dock is empty' }).waitFor();
  await page.reload();
  await page.getByRole('heading', { name: 'Main dock is empty' }).waitFor();
  await btn('Open Overview').click();
  await panel('Overview').waitFor();
  await inject('{invalid json');
  await page.reload();
  await panel('Overview').waitFor();
  assert.deepEqual(await mainTabs.getByRole('tab').allTextContents(), ['Overview', 'Applications']);
  assert.deepEqual(errors, []);
  console.log('Docking smoke passed: keyboard and pointer docking, resize, focus and compact layout, reload and reset, retained Watch/Scan, cross-panel requests, and ordered Activity log.');
} finally {
  await browser.close();
}
