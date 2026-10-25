import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { startHarness, root } from './server.mjs';
import { openChrome } from './chrome.mjs';
import { effectReference } from './effect-reference.mjs';

const { values } = parseArgs({ options: {
  fixtures: { type: 'string' }, 'engine-assets': { type: 'string' },
  output: { type: 'string' }, hardware: { type: 'boolean' },
  effect: { type: 'string' },
} });
assert(values.fixtures && values['engine-assets'], '--fixtures and --engine-assets are required');
const fixtures = JSON.parse(await readFile(path.join(values.fixtures, 'fixtures.json'), 'utf8'));
const effect = values.effect || fixtures.effect || 'foliagesway';
assert(['foliagesway', 'waterripple', 'chain'].includes(effect), 'Unsupported reference effect');
const { cases, ...images } = fixtures;
const output = path.resolve(values.output || path.join(root, 'artifacts', `${effect}-browser-${Date.now()}`));
await mkdir(output);
const digest = data => createHash('sha256').update(data).digest('hex');
const sources = {};
for (const kind of effect === 'chain' ? ['foliagesway', 'waterripple'] : [effect]) {
  const shaders = path.join(values['engine-assets'], `effects/${kind}/shaders/effects`);
  const [vertex, fragment] = await Promise.all(['vert', 'frag'].map(ext => readFile(path.join(shaders, `${kind}.${ext}`), 'utf8')));
  sources[kind] = { vertex, fragment };
}
const difference = (actual, expected) => {
  assert.equal(actual.length, expected.length);
  let maximum = 0, bad = 0;
  for (let i = 0; i < actual.length; i++) {
    const delta = Math.abs(actual[i] - expected[i]);
    maximum = Math.max(maximum, delta);
    if (delta > 2) bad++;
  }
  return { maximum_channel_error: maximum, channels_over_two: bad };
};
const report = { passed: false, effect, node: process.version, cases: [], shader_sha256: Object.fromEntries(Object.entries(sources).map(([kind, source]) => [kind, { vertex: digest(source.vertex), fragment: digest(source.fragment) }])),
  limits: 'Installed GLSL effect pass executed in WebGL, not the complete Wallpaper Engine runtime. Controlled captures do not prove displayed cadence.' };
let browser, harness;
try {
  for (const fixture of cases) {
    harness = await startHarness({ scene: fixture.package, engineAssets: values['engine-assets'], software: !values.hardware, output: path.join(output, fixture.name) });
    if (!browser) browser = await openChrome(harness);
    const page = browser.context.pages()[0];
    if (page.url() !== harness.url) await page.goto(harness.url);
    const initial = await page.evaluate(() => window.wallpaperTest.ready);
    assert.equal(initial.frame.adapter.software, !values.hardware);
    if (effect === 'chain') {
      assert.deepEqual(initial.report.effect_chains, { 1: fixture.chain.map(pass => pass.effect) });
      assert.equal(initial.report.omissions.filter(v => v.includes('effect') && v.includes('omitted')).length,
        fixture.unsupported || fixture.missing ? 1 : 0);
    } else {
      assert.deepEqual(initial.report.image_motions, { 1: effect });
      assert(!initial.report.omissions.some(v => v.includes(effect)));
    }
    await page.evaluate(({ width, height }) => window.wallpaperTest.resize(width, height), { width: fixtures.width, height: fixtures.height });
    const results = [];
    let first, firstPixels;
    const reference = async (time, chain = fixture.chain) => {
      if (effect !== 'chain') return page.evaluate(effectReference, { ...sources[effect], ...images, ...fixture, effect, time });
      let rgba = fixture.rgba;
      let result;
      for (const pass of chain) {
        result = await page.evaluate(effectReference, { ...sources[pass.effect], ...images, ...fixture, ...pass, rgba, time, raw: true });
        rgba = Buffer.from(result).toString('base64');
      }
      for (let i = 0; i < result.length; i += 4) {
        const alpha = result[i + 3] / 255 * (fixture.opacity ?? 1);
        for (let c = 0; c < 3; c++) result[i + c] = Math.round(Math.min(255, result[i + c] * (fixture.brightness ?? 1)) * alpha);
        result[i + 3] = 255;
      }
      return result;
    };
    for (const time of [0, 500, 1256, 1257, 1500, 4000, 1000000, 0]) {
      const state = await page.evaluate(time => window.wallpaperTest.seek(time), time);
      const actual = await page.evaluate(() => { const c = document.querySelector('#wallpaper'); return [...c.getContext('2d').getImageData(0, 0, c.width, c.height).data]; });
      const expected = await reference(time);
      const delta = difference(actual, expected);
      results.push({ time_ms: time, native_sha256: state.frame.sha256, ...delta });
      if (first === undefined) { first = state.frame.sha256; firstPixels = actual; }
      if (time === 0) assert.equal(state.frame.sha256, first, 'Exact time-zero rewind');
      if (time === 1500 || delta.channels_over_two) {
        await page.locator('#wallpaper').screenshot({ path: path.join(harness.output, `native-${time}.png`) });
        await page.evaluate(({ expected, width, height }) => {
          document.querySelector('#foliage-oracle')?.remove();
          const c = document.createElement('canvas'); c.id = 'foliage-oracle'; c.width = width; c.height = height;
          c.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(expected), width, height), 0, 0);
          document.querySelector('main').append(c);
        }, { expected, width: fixtures.width, height: fixtures.height });
        await page.locator('#foliage-oracle').screenshot({ path: path.join(harness.output, `reference-${time}.png`) });
      }
      assert.equal(delta.channels_over_two, 0, `${fixture.name} at ${time}ms: ${JSON.stringify(delta)}`);
      if (fixture.name === 'rotated' && time === 1500) {
        assert(difference(firstPixels, expected).channels_over_two > 0, 'The oracle must reject frozen motion');
        const damaged = actual.slice(); damaged[0] = actual[0] > 127 ? 0 : 255;
        assert(difference(damaged, expected).channels_over_two > 0, 'The oracle must reject corrupt pixels');
        report.fault_checks = ['frozen-motion rejected', 'corrupt pixels rejected'];
        if (effect === 'chain') {
          assert(difference(await reference(time, [...fixture.chain].reverse()), expected).channels_over_two > 0, 'Reversed passes must fail');
          assert(difference(await reference(time, fixture.chain.slice(0, 1)), expected).channels_over_two > 0, 'Missing pass must fail');
          report.fault_checks.push('reversed order rejected', 'missing pass rejected');
        }
      }
    }
    report.cases.push({ name: fixture.name, adapter: initial.frame.adapter, renderer_sha256: initial.renderer_sha256, results });
    const cleanup = await harness.close(); harness = null;
    assert.equal(cleanup.code, 0);
  }
  assert.equal(report.fault_checks.length, effect === 'chain' ? 4 : 2);
  report.passed = true;
} catch (e) { report.error = e.stack; process.exitCode = 1; }
finally {
  await browser?.context.close();
  if (harness) await harness.close();
  await writeFile(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ passed: report.passed, cases: report.cases.length, output, error: report.error }));
}
