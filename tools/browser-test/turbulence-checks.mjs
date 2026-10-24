import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';

// The generated browser-turbulence fixture uses a fixed 8 units/s² force.
// Its independent displacement oracle is 4*t²; exact engine noise is untested.
export async function verifyTurbulence(page, output) {
  await page.evaluate(() => window.wallpaperTest.resize(1024, 512));
  const initial = await page.evaluate(() => window.wallpaperTest.seek(0));
  assert.deepEqual(initial.report.particle_objects, [2, 3, 4, 5, 6]);
  assert.equal(initial.report.omissions.filter(v => v.includes('turbulence uses independent harmonic noise')).length, 5);
  assert(!initial.report.omissions.some(v => v.includes('operator component turbulence is omitted')));
  const checkpoints = [];
  for (const time of [0, 1000, 2000, 4000, 0]) {
    const state = await page.evaluate(time => window.wallpaperTest.seek(time), time);
    const t = time / 1000;
    const checks = [
      { point: [(64 + 4 * t * t) * 2, 128], channels: [true, false, false] },
      { point: [384, 128], channels: [false, true, false] },
      { point: [(320 + 2 * t * t) * 2, 128], channels: [false, false, true] },
      { point: [(448 - 4 * t * t) * 2, 128], channels: [true, true, false] },
    ];
    const pixels = await page.evaluate(checks => checks.map(({ point }) => window.wallpaperTest.pixel(...point)), checks);
    for (const [i, pixel] of pixels.entries()) {
      assert.equal(pixel[3], 255);
      checks[i].channels.forEach((bright, channel) => assert(bright ? pixel[channel] > 240 : pixel[channel] < 40,
        `Turbulence pixel at ${time}ms, particle ${i}: ${JSON.stringify(pixel)}`));
    }
    checkpoints.push({ time_ms: time, sha256: state.frame.sha256, checks, pixels });
    await page.locator('canvas').screenshot({ path: path.join(output, `turbulence-${checkpoints.length}-${time}.png`) });
  }
  assert.equal(checkpoints[0].sha256, checkpoints[4].sha256);
  assert.notEqual(checkpoints[0].sha256, checkpoints[1].sha256);
  const positions = [];
  for (let sample = 0; sample <= 120; sample++) {
    const time = 2000 + Math.round(sample * 1000 / 120);
    await page.evaluate(time => window.wallpaperTest.seek(time), time);
    const center = await page.evaluate(() => {
      const canvas = document.querySelector('canvas');
      const { data } = canvas.getContext('2d').getImageData(0, 120, 300, 16);
      let sum = 0, count = 0;
      for (let i = 0; i < data.length; i += 4) {
        if (data[i] > 240 && data[i + 1] < 40 && data[i + 2] < 40) { sum += i / 4 % 300; count++; }
      }
      return count ? sum / count : null;
    });
    assert.notEqual(center, null, 'Moving red particle must remain visible');
    const expected = (64 + 4 * (time / 1000) ** 2) * 2 - 0.5;
    assert(Math.abs(center - expected) <= 1, 'Raster position must track the independent continuous path');
    positions.push({ time_ms: time, x: center });
  }
  assert(new Set(positions.map(p => p.x)).size > 30, 'Motion must not remain on a low-rate frame grid');
  assert(positions.slice(1).every((p, i) => Math.abs(p.x - positions[i].x) <= 2));
  const result = { passed: true, checkpoints, positions, limits: 'Controlled samples prove pixels and sampling continuity, not display cadence or Wallpaper Engine noise fidelity.' };
  await writeFile(path.join(output, 'turbulence.json'), JSON.stringify(result, null, 2));
  return result;
}
