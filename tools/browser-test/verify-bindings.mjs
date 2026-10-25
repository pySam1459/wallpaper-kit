import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { startHarness, root } from './server.mjs';
import { openChrome } from './chrome.mjs';

const { values } = parseArgs({ options: { fixtures: { type: 'string' }, 'engine-assets': { type: 'string' }, output: { type: 'string' }, hardware: { type: 'boolean' } } });
assert(values.fixtures && values['engine-assets'], '--fixtures and --engine-assets are required');
const fixtures = JSON.parse(await readFile(path.join(values.fixtures, 'fixtures.json'), 'utf8'));
const source = await readFile(path.join(root, 'src/scene/text.rs'), 'utf8');
const output = path.resolve(values.output || path.join(root, 'artifacts', `bindings-browser-${Date.now()}`));
await mkdir(output);
const report = { passed: false, cases: [], limits: 'Checked installed scripts run only in isolated Chrome as a formatting oracle. Core playback does not execute JavaScript. Browser readback does not establish native display cadence.' };
let browser, harness;
try {
  for (const fixture of fixtures.cases) {
    const digest = createHash('sha256').update(fixture.script).digest('hex');
    assert.equal(digest, fixture.digest);
    assert(source.includes(`"${digest}"`), 'Only explicitly checked profiles may execute in this reference');
    harness = await startHarness({ scene: fixture.package, engineAssets: values['engine-assets'], software: !values.hardware, output: path.join(output, fixture.name) });
    if (!browser) browser = await openChrome(harness);
    const page = browser.context.pages()[0];
    if (page.url() !== harness.url) await page.goto(harness.url);
    const initial = await page.evaluate(() => window.wallpaperTest.ready);
    assert.equal(initial.frame.adapter.software, !values.hardware);
    assert.deepEqual(initial.report.text_objects, [2]);
    assert(!initial.report.omissions.some(v => v.includes('binding uses') || v.includes('optional content omitted')));
    await page.evaluate(() => window.wallpaperTest.resize(256, 128));
    await page.evaluate(() => window.wallpaperTest.setProperties({ tint: '0 1 0' }));
    const results = [];
    for (const time of [Date.UTC(2026, 9, 25, 0, 4, 5), Date.UTC(2026, 9, 25, 12, 34, 56), Date.UTC(2026, 9, 25, 23, 59, 59), Date.UTC(2026, 9, 26), Date.UTC(2024, 1, 29, 17, 8, 9), Date.UTC(2026, 9, 25, 0, 4, 5)]) {
      const phase = results.length === 5 ? 0 : results.length;
      const changes = { clock: true, static: false, format24: phase % 2 === 0, seconds: phase % 3 !== 1 };
      const options = Object.fromEntries(Object.entries(fixture.scriptproperties).map(([key, value]) => [key,
        typeof value === 'object' && value !== null && value.user ? changes[value.user] ?? value.value : value]));
      const expected = await page.evaluate(({ script, properties, time }) => {
        const defaults = {};
        const builder = {};
        for (const method of ['addCheckbox', 'addText', 'addCombo']) builder[method] = option => {
          defaults[option.name] = option.value ?? option.options?.[0]?.value;
          return builder;
        };
        builder.finish = () => ({ ...defaults, ...properties });
        class FixedDate extends Date {
          constructor(...args) { super(...(args.length ? args : [time])); }
          static now() { return time; }
        }
        return new Function('createScriptProperties', 'Date', script.replace(/\bexport\s+/g, '') + '\nreturn update();')(() => builder, FixedDate);
      }, { script: fixture.script, properties: options, time });
      await page.evaluate(changes => window.wallpaperTest.setProperties(changes), changes);
      const clock = await page.evaluate(time => window.wallpaperTest.setCalendarTime(time), time);
      assert.equal(clock.frame.text_values['2'], expected, 'Native formatting must match the installed script');
      const literal = await page.evaluate(caption => window.wallpaperTest.setProperties({ clock: false, static: true, caption }), expected);
      assert.equal(literal.frame.sha256, clock.frame.sha256, 'Clock glyph pixels must equal the independent literal text');
      assert(literal.pool.resident_textures <= 3);
      assert(literal.pool.source_texture_bytes < 16 * 1024 * 1024);
      results.push({ unix_ms: time, text: expected, sha256: clock.frame.sha256 });
    }
    assert.equal(results[0].sha256, results.at(-1).sha256, 'Exact calendar and option rewind');
    assert(results.some(r => r.text !== results[0].text), 'Frozen text must fail the oracle');
    const pixels = await page.evaluate(() => [...document.querySelector('#wallpaper').getContext('2d').getImageData(0, 0, 256, 128).data]);
    assert(pixels.some((value, i) => i % 4 === 1 && value > 30), 'Visible green glyphs');
    assert(pixels.every((value, i) => i % 4 !== 0 || value <= 10), 'Tint must apply, including blurred glyphs');
    await page.locator('#wallpaper').screenshot({ path: path.join(harness.output, 'verified-text.png') });
    const before = await page.evaluate(() => window.wallpaperTest.inspect());
    await assert.rejects(page.evaluate(() => window.wallpaperTest.setProperties({ clock: 'invalid' })));
    await assert.rejects(page.evaluate(() => window.wallpaperTest.setProperties({ tint: 'invalid color' })));
    if ('use24hFormat' in fixture.scriptproperties) await assert.rejects(page.evaluate(() => window.wallpaperTest.setProperties({ format24: 'invalid' })));
    const after = await page.evaluate(() => window.wallpaperTest.capture());
    assert.equal(after.frame.sha256, before.frame.sha256, 'Invalid update retains previous pixels');
    assert.deepEqual(after.report.user_properties, before.report.user_properties);
    const hidden = await page.evaluate(() => window.wallpaperTest.setProperties({ base: false, clock: false, static: false }));
    assert.deepEqual(hidden.report.object_ids, []);
    assert.deepEqual(hidden.frame.text_values, {});
    assert.deepEqual(await page.evaluate(() => window.wallpaperTest.pixel(128, 64)), [16, 24, 32, 255]);
    const cleanup = await harness.close(); harness = null;
    assert.equal(cleanup.code, 0);
    report.cases.push({ name: fixture.name, script_sha256: digest, renderer_sha256: initial.renderer_sha256, adapter: initial.frame.adapter, results, fault_checks: ['frozen text rejected', 'invalid update retained pixels', 'all hidden clears'] });
  }
  assert.equal(report.cases.length, 9);
  report.passed = true;
} catch (error) { report.error = error.stack; process.exitCode = 1; }
finally {
  await browser?.context.close();
  if (harness) await harness.close();
  await writeFile(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, output, error: report.error }));
}
