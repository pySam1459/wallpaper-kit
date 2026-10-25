// Browser consumer proxy, separate from native throughput/resource benchmarks.
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { performance } from 'node:perf_hooks';
import { startHarness } from './server.mjs';
import { openChrome } from './chrome.mjs';

const { values } = parseArgs({ options: {
  scene: { type:'string' }, 'engine-assets':{ type:'string' }, output:{ type:'string' },
} });
assert(values.scene && values['engine-assets'] && values.output,'scene, engine-assets and output are required');
const distribution = values => {
  if (!values.length) return null;
  const sorted=[...values].sort((a,b)=>a-b);
  const at=p=>sorted[Math.max(0,Math.ceil(sorted.length*p)-1)];
  return {count:values.length,mean:values.reduce((a,b)=>a+b,0)/values.length,p50:at(.5),p95:at(.95),p99:at(.99),max:sorted.at(-1)};
};
const harness=await startHarness({scene:values.scene,engineAssets:values['engine-assets'],software:false,output:values.output});
let browser;
const report={passed:false,phases:[],limits:[
  'Headless Chrome debug consumer with PNG transport, not T3 or native monitor presentation.',
  'Paused versus playing retains the same loaded renderer. Browser keyboard round trips include Playwright/CDP overhead.',
  'Event-to-handler delay and animation-frame gaps describe this page, not end-to-end device input latency.',
]};
try {
  browser=await openChrome(harness);
  const page=browser.context.pages()[0];
  await page.evaluate(()=>window.wallpaperTest.ready);
  const initial=await page.evaluate(()=>window.wallpaperTest.resize(1920,1080));
  report.adapter=initial.frame.adapter;
  report.renderer_sha256=initial.renderer_sha256;
  report.playback=initial.report;
  await page.evaluate(()=>{
    const input=document.createElement('textarea');input.id='benchmark-input';input.setAttribute('aria-label','Benchmark input');
    input.style.cssText='position:fixed;top:0;right:0;z-index:9999;width:280px;height:70px';document.body.append(input);
    window.probe={active:false,gaps:[],input:[],longTasks:[],last:null};
    input.addEventListener('input',event=>{if(window.probe.active)window.probe.input.push(performance.now()-event.timeStamp);});
    const frame=now=>{
      const p=window.probe;
      if(p.active && p.last!==null && p.gaps.length<4096)p.gaps.push(now-p.last);
      p.last=now;requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
    window.probe.longTasksAvailable=PerformanceObserver.supportedEntryTypes.includes('longtask');
    if(window.probe.longTasksAvailable)new PerformanceObserver(list=>{
      if(window.probe.active)for(const entry of list.getEntries())if(window.probe.longTasks.length<4096)window.probe.longTasks.push(entry.duration);
    }).observe({type:'longtask',buffered:false});
  });
  for (const playing of [false,true,false,true]) {
    await page.evaluate(playing=>playing?window.wallpaperTest.play():window.wallpaperTest.pause(),playing);
    await page.waitForTimeout(1000); // Warm transport and settle pause before observing.
    await page.locator('#benchmark-input').fill('');await page.locator('#benchmark-input').focus();
    const before=await page.evaluate(()=>window.wallpaperTest.inspect());
    await page.evaluate(()=>Object.assign(window.probe,{active:true,gaps:[],input:[],longTasks:[],last:null}));
    const started=performance.now(),roundTrips=[];
    for(let i=0;i<40;i++) {
      const due=started+i*200;
      if(performance.now()<due)await new Promise(resolve=>setTimeout(resolve,due-performance.now()));
      const keyStarted=performance.now();await page.keyboard.type('a');roundTrips.push(performance.now()-keyStarted);
    }
    const elapsed=(performance.now()-started)/1000;
    const probe=await page.evaluate(()=>{window.probe.active=false;return window.probe;});
    const after=await page.evaluate(()=>window.wallpaperTest.inspect());
    assert.equal(probe.input.length,40,'Every typed character must reach the page');
    assert.equal((await page.locator('#benchmark-input').inputValue()).length,40);
    if(!playing)assert.equal(after.pool.published_frames,before.pool.published_frames,'Paused renderer must remain idle');
    else assert(after.pool.published_frames>before.pool.published_frames,'Enabled renderer must advance');
    report.phases.push({playing,elapsed_seconds:elapsed,keyboard_round_trip_ms:distribution(roundTrips),
      event_to_handler_ms:distribution(probe.input),animation_frame_gap_ms:distribution(probe.gaps),
      animation_gaps_over_25ms:probe.gaps.filter(v=>v>25).length,long_tasks_available:probe.longTasksAvailable,
      long_task_ms:distribution(probe.longTasks),native_completed_frames:after.pool.published_frames-before.pool.published_frames});
    await writeFile(path.join(harness.output,`responsiveness-${report.phases.length}.json`),JSON.stringify({roundTrips,...probe},null,2));
  }
  await page.evaluate(()=>window.wallpaperTest.pause());
  report.passed=true;
} catch(error) {report.error=error.stack;process.exitCode=1;}
finally {
  await browser?.context.close();
  report.cleanup=await harness.close();
  if(report.cleanup.code!==0){report.passed=false;process.exitCode=1;}
  await writeFile(path.join(harness.output,'responsiveness.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify({passed:report.passed,output:harness.output,error:report.error}));
}
