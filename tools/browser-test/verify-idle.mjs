// Consumer focus/still transitions through the real renderer, without desktop UI.
import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import path from 'node:path';
import {parseArgs} from 'node:util';
import {startHarness} from './server.mjs';
import {openChrome} from './chrome.mjs';

const {values} = parseArgs({options:{scene:{type:'string'},'engine-assets':{type:'string'},output:{type:'string'}}});
const harness = await startHarness({scene:values.scene,engineAssets:values['engine-assets'],software:false,output:values.output});
let browser;
const report={passed:false,checks:[],limits:['Headless Chrome PNG consumer; no production host, native monitor cadence or zero-copy claim.','Focus callbacks are synthesized; actual desktop focus delivery is unverified.']};
try {
  browser=await openChrome(harness);
  const page=browser.context.pages()[0];
  await page.evaluate(()=>window.wallpaperTest.ready);
  const waitState=async predicate=>{
    const deadline=Date.now()+30000;
    while(Date.now()<deadline){
      const state=await page.evaluate(()=>window.wallpaperTest.inspect());
      if(predicate(state))return state;
      await page.waitForTimeout(50);
    }
    throw Error('Consumer idle/resume state timed out');
  };
  await page.evaluate(()=>window.wallpaperTest.play());
  const start=await page.evaluate(()=>window.wallpaperTest.inspect());
  await waitState(s=>s.frame.sequence>start.frame.sequence);
  await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
  const still=await waitState(s=>!s.playing && s.pool?.source_texture_bytes===0 && s.pool.effect_texture_bytes===0 && s.pool.resident_textures===1);
  const pixels=await page.evaluate(()=>document.querySelector('canvas').dataset.sha256);
  await page.waitForTimeout(400);
  const idle=await page.evaluate(()=>window.wallpaperTest.inspect());
  assert.equal(idle.pool.published_frames,still.pool.published_frames);
  assert.equal(await page.evaluate(()=>document.querySelector('canvas').dataset.sha256),pixels);
  report.still=still.pool;
  report.checks.push('consumer blur callback freezes pixels, stops publication and releases idle GPU resources');
  await page.evaluate(()=>window.dispatchEvent(new Event('focus')));
  await waitState(s=>s.playing && s.frame.sequence>still.frame.sequence);
  await page.evaluate(()=>window.wallpaperTest.pause());
  await waitState(s=>!s.playing && s.pool?.source_texture_bytes===0 && s.pool.effect_texture_bytes===0);
  await page.evaluate(()=>window.dispatchEvent(new Event('blur')));
  await page.evaluate(()=>window.dispatchEvent(new Event('focus')));
  await page.waitForTimeout(200);
  assert.equal((await page.evaluate(()=>window.wallpaperTest.inspect())).playing,false,'Manual pause must survive focus changes');
  const resized=await page.evaluate(()=>window.wallpaperTest.resize(317,193));
  assert.equal(resized.frame.width,317);
  assert.equal(resized.frame.height,193);
  await waitState(s=>!s.playing && s.pool?.source_texture_bytes===0 && s.pool.effect_texture_bytes===0);
  report.checks.push('focus resumes requested playback; explicit pause survives focus; still resize redraws and trims again');
  await page.screenshot({path:path.join(harness.output,'still-resized.png')});
  report.passed=true;
} catch(error){report.error=error.stack;process.exitCode=1;}
finally {
  await browser?.context.close();
  report.cleanup=await harness.close();
  if(report.cleanup.code!==0){report.passed=false;process.exitCode=1;}
  await writeFile(path.join(harness.output,'idle.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify({passed:report.passed,output:harness.output,error:report.error}));
}
