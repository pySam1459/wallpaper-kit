import { chromium } from 'playwright-core';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

export async function openChrome(harness, headless = true) {
  const profile = path.join(harness.output, 'chrome-profile');
  const context = await chromium.launchPersistentContext(profile, {
    channel: 'chrome', headless, viewport: { width: 1120, height: 1000 },
    deviceScaleFactor: 1, args: ['--remote-debugging-port=0'],
  });
  try {
    await context.pages()[0].goto(harness.url);
    const [port] = (await readFile(path.join(profile, 'DevToolsActivePort'), 'utf8')).trim().split('\n');
    const browserUrl = `http://127.0.0.1:${port}`;
    await writeFile(path.join(harness.output, 'chrome.json'), JSON.stringify({ browser_url: browserUrl, profile }, null, 2));
    return { context, browserUrl };
  } catch (e) { await context.close(); throw e; }
}
