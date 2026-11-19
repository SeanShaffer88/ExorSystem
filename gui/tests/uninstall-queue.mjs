// Exercise serial review against the production UI. These IPC calls never start Windows processes.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { mockDesktop } from './ipc-fixture.mjs';

const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 820 } });
  page.setDefaultTimeout(10_000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await mockDesktop(page);
  await page.goto('https://exor.test/?uninstall-queue');
  const apps = page.getByRole('tabpanel', { name: 'Applications' });
  const inventory = apps.getByRole('region', { name: 'Application inventory' });
  const start = apps.getByRole('button', { name: 'Select apps for uninstall queue', exact: true });
  const queue = apps.getByRole('region', { name: 'Uninstall review queue' });
  const items = queue.getByRole('list', { name: 'Queued applications' });
  const dialog = page.getByRole('dialog');
  const search = inventory.getByRole('searchbox', { name: 'Search programs' });
  const next = queue.getByRole('button', { name: 'Review next uninstaller' });
  const review = dialog.getByRole('button', { name: 'Continue', exact: true });
  const run = dialog.getByRole('button', { name: 'Run uninstaller', exact: true });
  const approve = dialog.getByRole('checkbox', { name: 'Run this program’s own uninstaller' });
  const backToQueue = dialog.getByRole('button', { name: 'Return to queue', exact: true });
  const name = n => `Queue Lab ${String(n).padStart(2, '0')}`;
  const pick = (n, version = '1.0') => inventory.getByRole('checkbox', { name: `Queue ${name(n)} ${version}`, exact: true });
  const row = n => items.getByRole('listitem').filter({ hasText: name(n) });
  const calls = command => page.evaluate(cmd => window.exorCalls.filter(([c]) => c === cmd).map(([, args]) => args), command);
  const preview = async (n, attempt) => {
    await dialog.getByRole('heading', { name: name(n), exact: true }).waitFor();
    await dialog.getByText(`"preview ${attempt}"`, { exact: true }).waitFor();
    assert.equal(await dialog.getByRole('button', { name: 'Skip to leftover review' }).count(), 0);
  };
  const confirm = async () => {
    await review.click();
    assert(await run.isDisabled());
    await approve.check();
    await run.click();
  };
  const exit = async code => {
    await page.waitForFunction(() => typeof window.finishQueuedUninstaller === 'function');
    await page.evaluate(code => window.finishQueuedUninstaller(code), code);
  };
  const ready = async () => {
    await page.waitForFunction(() => ![...document.querySelectorAll('button')].find(b => b.textContent.trim() === 'Refresh inventory')?.disabled);
  };

  await page.getByRole('toolbar', { name: 'Workspace tools' }).getByRole('button', { name: 'Applications', exact: true }).click();
  await inventory.getByRole('button', { name: /Queue Lab 01/ }).waitFor();
  assert.equal(await queue.count(), 0, 'queue selection is opt-in');
  await start.click();
  await queue.getByText('No applications queued.', { exact: false }).waitFor();
  assert.equal(await inventory.getByRole('checkbox', { name: /Queue Alpha Package|Queue Per-user Fixture/ }).count(), 0);
  await pick(1).press('Space');
  assert(await pick(1).isChecked());
  assert.equal(await apps.getByRole('complementary', { name: 'Application details' }).count(), 0, 'checkbox does not open details');
  await inventory.getByRole('button', { name: 'Next', exact: true }).click();
  await pick(17).check();
  await search.fill(name(2));
  await pick(2).check();
  assert.equal(await items.getByRole('listitem').count(), 3, 'picks survive inventory paging and filtering');
  await start.click();
  assert.equal(await pick(2).count(), 0, 'leave selection mode without clearing the queue');
  assert.deepEqual(await calls('preview_uninstaller'), []);
  assert.deepEqual(await calls('run_uninstaller'), []);

  await next.press('Enter');
  await preview(1, 1);
  await page.keyboard.press('Escape');
  await dialog.waitFor({ state: 'hidden' });
  await row(1).getByText('Waiting for review', { exact: true }).waitFor();
  assert(await next.evaluate(el => el === document.activeElement), 'return keyboard focus to the initiating button');
  assert.deepEqual(await calls('run_uninstaller'), [], 'closing a preview does not run or skip an item');
  await next.click();
  await preview(1, 2);
  await confirm();
  await dialog.getByRole('progressbar', { name: 'Vendor uninstaller running' }).waitFor();
  assert(await dialog.getByRole('button', { name: 'Close wizard' }).isDisabled());
  await page.keyboard.press('Escape');
  assert(await dialog.isVisible(), 'Escape cannot close a running vendor review');
  await exit(1602);
  await backToQueue.click();
  await row(1).getByText('Exit code 1602', { exact: true }).waitFor();
  assert.equal((await calls('preview_uninstaller')).length, 2, 'next preview is never started automatically');
  assert.equal((await calls('run_uninstaller')).length, 1);

  await row(1).getByRole('button', { name: /Review .* again/ }).click();
  await preview(1, 3);
  await dialog.getByText(/This is a fresh preview, not a replay/).waitFor();
  await page.evaluate(() => { window.queueStale = 'queue-1'; });
  await confirm();
  await dialog.getByRole('alert').getByText(/registered uninstaller changed/).waitFor();
  assert.equal(await run.count(), 0, 'a failure cannot replay in the same dialog');
  await backToQueue.click();
  await row(1).getByText('Outcome unconfirmed', { exact: true }).waitFor();
  await page.evaluate(() => { window.queueStale = null; });
  await row(1).getByRole('button', { name: /Review .* again/ }).click();
  await preview(1, 4);
  await confirm();
  await exit(0);
  await dialog.getByText(/Check Windows to confirm the app was removed/).waitFor();
  await backToQueue.click();
  await row(1).getByText('Exit code 0', { exact: true }).waitFor();
  assert.equal(await row(1).getByRole('button', { name: /Review .* again/ }).count(), 0);
  assert.equal(await row(1).getByText('Removed', { exact: true }).count(), 0, 'an exit code is not proof of uninstall');

  await row(17).getByRole('button', { name: `Skip ${name(17)}` }).click();
  await row(17).getByText('Skipped', { exact: true }).waitFor();
  await page.evaluate(() => { window.queueBlocked = 'queue-2'; });
  await next.click();
  await dialog.getByRole('heading', { name: name(2), exact: true }).waitFor();
  await dialog.getByRole('alert').getByText(/No supported live registration/).waitFor();
  assert(await review.isDisabled());
  await backToQueue.click();
  await row(2).getByText('Preview blocked', { exact: true }).waitFor();
  const attempts = await calls('run_uninstaller');
  assert.deepEqual(attempts.map(a => [a.programId, a.quiet, a.expected.args[1]]), [
    ['queue-1', false, 'preview 2'], ['queue-1', false, 'preview 3'], ['queue-1', false, 'preview 4'],
  ]);

  await page.evaluate(() => { window.queueChanged = 'queue-17'; window.queueMissing = 'queue-2'; });
  await apps.getByRole('button', { name: 'Refresh inventory', exact: true }).click();
  await row(17).getByText(/Inventory details changed/).waitFor();
  await row(2).getByText(/No matching eligible registration/).waitFor();
  assert(await row(17).getByRole('button', { name: /Review .* again/ }).isDisabled());
  assert(await row(2).getByRole('button', { name: /Review .* again/ }).isDisabled());
  await row(17).getByRole('button', { name: /Remove .* from queue/ }).click();
  await start.click();
  await search.fill(name(17));
  await pick(17, '2.0').check();
  await row(17).getByText(/2\.0/).waitFor();
  assert.deepEqual(await calls('run_uninstaller'), attempts, 'inventory refresh and explicit reselection do not launch');

  await queue.getByRole('button', { name: 'Clear queue' }).click();
  await search.fill('');
  await ready();
  for (let n = 3; n <= 14; n++) await pick(n).check();
  await queue.getByText('12 / 12 queued', { exact: true }).waitFor();
  await search.fill(name(15));
  assert(await pick(15).isDisabled());
  await page.setViewportSize({ width: 390, height: 590 });
  await queue.getByText('12 / 12 queued', { exact: true }).waitFor();
  assert(await apps.evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'queued reviews overflowed a compact dock');
  assert.deepEqual(await calls('find_leftovers'), []);
  assert.deepEqual(await calls('prepare_cleanup'), []);
  assert.deepEqual(await calls('apply_cleanup'), []);
  await page.reload();
  await page.getByRole('toolbar', { name: 'Workspace tools' }).getByRole('button', { name: 'Applications', exact: true }).click();
  await inventory.getByRole('button', { name: /Queue Lab 01/ }).waitFor();
  assert.equal(await queue.count(), 0, 'the queue is never restored after reload');
  assert.deepEqual(await calls('run_uninstaller'), []);

  // Signature review applies to every queued item: unverified needs a second confirmation,
  // blocked never runs.
  await page.evaluate(() => { window.queueSignature = { 'queue-3': 'unsigned', 'queue-4': 'rejected' }; });
  const trust = dialog.getByRole('checkbox', { name: 'I trust this uninstaller even though its signature is not verified' });
  await ready();
  await start.click();
  await pick(3).check();
  await pick(4).check();
  await start.click();
  await next.click();
  await preview(3, 1);
  await dialog.getByText('Unsigned', { exact: true }).waitFor();
  await review.click();
  await approve.check();
  assert(await run.isDisabled(), 'an unsigned uninstaller needs the extra confirmation');
  await trust.check();
  await run.click();
  await exit(0);
  await backToQueue.click();
  await row(3).getByText('Exit code 0', { exact: true }).waitFor();
  const unsigned = await calls('run_uninstaller');
  assert.equal(unsigned.length, 1);
  assert.equal(unsigned[0].confirmUnverified, true);
  assert.equal(unsigned[0].expected.signature.status, 'unsigned');
  await next.click();
  await preview(4, 1);
  await dialog.getByText('Blocked', { exact: true }).waitFor();
  await review.click();
  await dialog.getByText('ExorSystem will not run this uninstaller', { exact: true }).waitFor();
  assert(await approve.isDisabled(), 'a blocked uninstaller cannot be approved');
  assert(await run.isDisabled());
  assert.equal(await trust.count(), 0, 'no trust override for a blocked uninstaller');
  await dialog.getByRole('button', { name: 'Close wizard' }).click();
  await row(4).getByText('Waiting for review', { exact: true }).waitFor();
  assert.equal((await calls('run_uninstaller')).length, 1, 'the blocked item was never run');
  assert.deepEqual(errors, []);
  console.log('Uninstall queue passed: multi-page picks, per-item review, signature review, running lockout, blocked/stale outcomes, fresh retry, skip, capacity, keyboard focus and no automatic cleanup.');
} finally {
  await browser.close();
}
