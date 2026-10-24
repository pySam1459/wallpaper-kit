// MCP entry point: owns a test renderer, an isolated Chrome and the official DevTools server.
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { openChrome } from './chrome.mjs';
import { startHarness } from './server.mjs';

const { values } = parseArgs({ options: {
  scene: { type: 'string' }, 'engine-assets': { type: 'string' }, output: { type: 'string' },
  software: { type: 'boolean' }, headed: { type: 'boolean' },
} });
const harness = await startHarness({ scene: values.scene, engineAssets: values['engine-assets'], software: values.software, output: values.output });
let chrome;
let server;
let closing;
async function close() {
  return closing ||= (async () => {
    if (server && server.exitCode === null) server.kill();
    await chrome?.context.close();
    await harness.close();
  })();
}
try {
  chrome = await openChrome(harness, !values.headed);
  server = spawn(process.execPath, [
    fileURLToPath(new URL('node_modules/chrome-devtools-mcp/build/src/bin/chrome-devtools-mcp.js', import.meta.url)),
    `--browser-url=${chrome.browserUrl}`, '--no-usage-statistics', '--no-performance-crux',
  ], { windowsHide: true, stdio: ['pipe', 'pipe', 'inherit'], env: { ...process.env, CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS: '1' } });
  server.on('error', e => { console.error(e); process.exitCode = 1; close().catch(console.error); });
  server.on('close', code => { if (code && !closing) process.exitCode = code; close().catch(console.error); });
  server.stdin.on('error', e => { if (!closing) console.error(e); });
  process.stdin.pipe(server.stdin);
  server.stdout.pipe(process.stdout);
  process.stdin.on('end', () => { close().catch(console.error); });
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => { close().catch(console.error); });
  console.error(`wallpaper-kit test page: ${harness.url}\nEvidence: ${harness.output}`);
} catch (e) { await close(); throw e; }
