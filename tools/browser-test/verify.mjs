import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from 'playwright-core';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { ListRootsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { startHarness } from './server.mjs';
import { verifyTurbulence } from './turbulence-checks.mjs';

const { values } = parseArgs({ options: {
  scene: { type: 'string' }, 'engine-assets': { type: 'string' },
  hardware: { type: 'boolean' }, output: { type: 'string' },
  'turbulence-fixture': { type: 'boolean' },
} });
const harness = await startHarness({ scene: values.scene, engineAssets: values['engine-assets'], software: !values.hardware, output: values.output });
let context;
let mcp;
let failedHarness;
let entryClient;
let report = { passed: false, output: harness.output, checks: [], limits: ['PNG readback/browser transport does not establish native display cadence, zero-copy sharing or Wallpaper Engine fidelity.'] };
const consoleMessages = [];
const pageErrors = [];
let mcpStderr = '';
const response = (route, options) => fetch(harness.origin + route, { ...options, headers: { 'X-Wallpaper-Token': harness.token, ...options?.headers } });
try {
  context = await chromium.launchPersistentContext(path.join(harness.output, 'chrome-profile'), {
    channel: 'chrome', headless: true, viewport: { width: 1120, height: 1000 },
    deviceScaleFactor: 1, args: ['--remote-debugging-port=0'],
  });
  await context.tracing.start({ screenshots: true, snapshots: true, sources: true });
  const page = context.pages()[0];
  page.on('console', message => consoleMessages.push({ type: message.type(), text: message.text() }));
  page.on('pageerror', e => pageErrors.push(e.message));
  await page.goto(harness.url);
  let initial = await page.evaluate(() => window.wallpaperTest.ready);
  if (!initial.fixture) initial = await page.evaluate(() => window.wallpaperTest.setCalendarTime(Date.UTC(2026, 9, 25, 13, 4, 5)));
  assert(initial.ready && initial.frame.sha256);
  assert.equal(initial.frame.adapter.software, !values.hardware, 'Use the explicitly selected rendering adapter');
  assert.match(initial.renderer_sha256, /^[a-f0-9]{64}$/);
  const browserHash = await page.evaluate(async () => {
    const canvas = document.querySelector('canvas');
    const bytes = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
    return [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(v => v.toString(16).padStart(2, '0')).join('');
  });
  assert.equal(browserHash, initial.frame.sha256, 'Browser canvas must contain the real native RGBA pixels');
  if (initial.fixture) {
    assert.deepEqual(await page.evaluate(() => window.wallpaperTest.pixel(40, 100)), [255, 0, 0, 255]);
    await page.getByRole('spinbutton', { name: 'Time (ms)' }).fill('125');
    await page.getByRole('button', { name: 'Seek', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('canvas').dataset.timeMs === '125');
    assert.deepEqual(await page.evaluate(() => window.wallpaperTest.pixel(40, 100)), [0, 255, 0, 255]);
    report.checks.push('independent GPU fixture pixels at two sprite phases');
  } else {
    await page.locator('canvas').screenshot({ path: path.join(harness.output, 'scene-0.png') });
    await page.evaluate(() => window.wallpaperTest.seek(1500));
    await page.locator('canvas').screenshot({ path: path.join(harness.output, 'scene-1500.png') });
  }
  const rewind = await page.evaluate(() => window.wallpaperTest.seek(0));
  assert.equal(rewind.frame.sha256, initial.frame.sha256);
  report.checks.push('native RGBA equals browser canvas; exact seek and rewind');
  if (values['turbulence-fixture']) {
    report.turbulence = await verifyTurbulence(page, harness.output);
    report.checks.push('independent turbulence pixels, masks, 120 Hz sampling and rewind');
    await page.evaluate(() => window.wallpaperTest.resize(640, 360));
    await page.evaluate(() => window.wallpaperTest.seek(0));
  }

  await page.getByRole('button', { name: 'Play', exact: true }).click();
  await page.waitForFunction(() => Number(document.querySelector('canvas').dataset.timeMs) > 150);
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  const paused = await page.evaluate(() => window.wallpaperTest.inspect());
  await page.waitForTimeout(250); // Observe idle behavior across several transport slots.
  const idle = await page.evaluate(() => window.wallpaperTest.inspect());
  assert.equal(idle.frame.sha256, paused.frame.sha256);
  assert.equal(idle.pool.published_frames, paused.pool.published_frames);
  assert.equal(idle.playing, false);
  assert(idle.pool.resident_textures <= 3);
  report.checks.push('live advancement, freeze-last-frame pause, idle and bounded GPU pool');

  await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('317');
  await page.getByRole('spinbutton', { name: 'Height', exact: true }).fill('193');
  await page.getByRole('button', { name: 'Resize', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('canvas').width === 317 && document.querySelector('canvas').height === 193);
  assert.equal((await page.evaluate(() => window.wallpaperTest.inspect())).frame.width, 317);
  const invalid = await response('/api/command', { method: 'POST', body: JSON.stringify({ name: 'resize', width: 0, height: 193 }) });
  assert.equal(invalid.status, 400);
  assert((await invalid.json()).error);
  assert((await page.evaluate(() => window.wallpaperTest.seek(0))).ready);
  report.checks.push('DOM resize controls and recovery after an invalid native command');

  assert.equal((await fetch(harness.origin + '/api/state')).status, 403);
  assert.equal((await response('/api/state', { headers: { Origin: 'https://example.invalid' } })).status, 403);
  assert.equal((await response('/api/command', { method: 'POST', body: JSON.stringify({ name: 'stop' }) })).status, 400);
  const burst = await Promise.all(Array.from({ length: 8 }, () => response('/api/frame', { method: 'POST' })));
  assert(burst.some(r => r.status === 429), 'Overlapping frame requests must be bounded');
  assert(burst.some(r => r.status === 200));
  await Promise.all(burst.map(r => r.arrayBuffer()));
  report.checks.push('session authentication, origin restriction and bounded frame admission');

  await page.screenshot({ path: path.join(harness.output, 'playwright.png'), fullPage: true });
  const [port] = (await readFile(path.join(harness.output, 'chrome-profile/DevToolsActivePort'), 'utf8')).trim().split('\n');
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [fileURL('node_modules/chrome-devtools-mcp/build/src/bin/chrome-devtools-mcp.js'), `--browser-url=http://127.0.0.1:${port}`, '--no-usage-statistics', '--no-performance-crux'],
    env: { ...process.env, CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS: '1' }, stderr: 'pipe',
  });
  transport.stderr.on('data', data => { mcpStderr = (mcpStderr + data).slice(-16384); });
  mcp = new Client({ name: 'wallpaper-kit-verifier', version: '1.0.0' }, { capabilities: { roots: {} } });
  mcp.setRequestHandler(ListRootsRequestSchema, () => ({ roots: [{ uri: pathToFileURL(harness.output).href, name: 'wallpaper-kit test evidence' }] }));
  await mcp.connect(transport);
  const tools = await mcp.listTools();
  assert(tools.tools.some(t => t.name === 'evaluate_script'));
  const pages = await mcp.callTool({ name: 'list_pages', arguments: {} });
  report.mcp_pages = pages;
  assert(!pages.isError, JSON.stringify(pages));
  const ownedPages = pages.content.filter(c => c.type === 'text').flatMap(c => c.text.split('\n')).filter(line => line.includes(`(${harness.url})`));
  assert.equal(ownedPages.length, 1, 'Select only the exact owned harness tab');
  const pageId = Number(ownedPages[0].match(/^(\d+):/)[1]);
  const evaluation = await mcp.callTool({ name: 'evaluate_script', arguments: { pageId, function: 'async () => { const before = await window.wallpaperTest.inspect(); const state = await window.wallpaperTest.setProperties({}); if (!before.playing && state.frame.sha256 !== before.frame.sha256) throw Error("Property preparation changed paused pixels"); return {title:document.title,state}; }' } });
  assert(!evaluation.isError, JSON.stringify(evaluation));
  assert(JSON.stringify(evaluation).includes('wallpaper-kit browser test'));
  report.checks.push('Chrome DevTools property replacement preserves paused pixels');
  const snapshot = await mcp.callTool({ name: 'take_snapshot', arguments: { pageId } });
  assert(!snapshot.isError, JSON.stringify(snapshot));
  const shot = await mcp.callTool({ name: 'take_screenshot', arguments: { pageId, filePath: path.join(harness.output, 'devtools.png') } });
  assert(!shot.isError, JSON.stringify(shot));
  await writeFile(path.join(harness.output, 'mcp.json'), JSON.stringify({ pages, evaluation, snapshot }, null, 2));
  report.checks.push('Chrome DevTools MCP attaches to the same isolated Chrome; evaluate, accessibility snapshot and screenshot');

  failedHarness = await startHarness({ scene: path.join(harness.output, 'missing.pkg'), engineAssets: path.join(harness.output, 'worker/engine'), software: true });
  const failurePage = await context.newPage();
  await failurePage.goto(failedHarness.url);
  await failurePage.waitForFunction(() => !document.querySelector('#error').hidden);
  assert(await failurePage.getByRole('alert').textContent());
  await failurePage.close();
  report.checks.push('renderer startup errors appear in the browser');
  entryClient = new Client({ name: 'wallpaper-kit-entry-verifier', version: '1.0.0' }, { capabilities: { roots: {} } });
  entryClient.setRequestHandler(ListRootsRequestSchema, () => ({ roots: [{ uri: pathToFileURL(harness.output).href, name: 'test evidence' }] }));
  const entryOutput = path.join(harness.output, 'mcp-entry');
  const entryArgs = [fileURL('mcp.mjs'), '--output', entryOutput];
  if (!values.hardware) entryArgs.push('--software');
  const entryTransport = new StdioClientTransport({ command: process.execPath, args: entryArgs, stderr: 'pipe' });
  entryTransport.stderr.on('data', data => { mcpStderr = (mcpStderr + data).slice(-16384); });
  await entryClient.connect(entryTransport);
  const entryPages = await entryClient.callTool({ name: 'list_pages', arguments: {} });
  assert(!entryPages.isError);
  const entryLines = entryPages.content.filter(c => c.type === 'text').flatMap(c => c.text.split('\n')).filter(line => /^\d+: wallpaper-kit browser test /.test(line));
  assert.equal(entryLines.length, 1);
  const entryPageId = Number(entryLines[0].match(/^(\d+):/)[1]);
  const entryEval = await entryClient.callTool({ name: 'evaluate_script', arguments: { pageId: entryPageId, function: 'async () => { const state = await window.wallpaperTest.ready; return {ready:state.ready, hash:state.frame.sha256}; }' } });
  assert(!entryEval.isError, JSON.stringify(entryEval));
  const entryText = entryEval.content.filter(c => c.type === 'text').map(c => c.text).join('\n');
  assert.match(entryText, /"ready"\s*:\s*true/);
  assert.match(entryText, /"hash"\s*:\s*"[a-f0-9]{64}"/);
  await entryClient.close(); entryClient = null;
  report.mcp_entry_cleanup = JSON.parse(await readFile(path.join(entryOutput, 'cleanup.json'), 'utf8'));
  assert.equal(report.mcp_entry_cleanup.code, 0);
  report.checks.push('standalone MCP entry point starts its own renderer/Chrome and cleans up on disconnect');
  assert.deepEqual(pageErrors, []);
  assert.equal(consoleMessages.filter(m => m.type === 'error').length, 0);
  report = { ...report, passed: true, scene: initial.report, frame: initial.frame, renderer_sha256: initial.renderer_sha256, node: process.version, chrome: context.browser()?.version() };
} catch (e) {
  report.error = e.stack;
  throw e;
} finally {
  await entryClient?.close();
  await mcp?.close();
  if (context) { await context.tracing.stop({ path: path.join(harness.output, 'trace.zip') }); await context.close(); }
  if (failedHarness) await failedHarness.close();
  report.cleanup = await harness.close();
  if (report.cleanup.code !== 0) { report.passed = false; process.exitCode = 1; }
  report.console = consoleMessages;
  report.page_errors = pageErrors;
  await writeFile(path.join(harness.output, 'report.json'), JSON.stringify(report, null, 2));
  await writeFile(path.join(harness.output, 'mcp-stderr.log'), mcpStderr);
  console.log(JSON.stringify({ passed: report.passed, output: harness.output, checks: report.checks, error: report.error }));
}

function fileURL(relative) { return fileURLToPath(new URL(relative, import.meta.url)); }
