const canvas = document.querySelector('#wallpaper');
const context = canvas.getContext('2d', { willReadFrequently: true });
const token = location.hash.slice(1);
let state;
let playing = false;
let wantsPlay = false;
let hostActive = true;
let pipeline = Promise.resolve();
let lastPoll = 0;
let framePending = false;
let animationFrame = 0;

function stopPolling() { playing = false; cancelAnimationFrame(animationFrame); }

async function request(route, body) {
  const response = await fetch(route, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { 'X-Wallpaper-Token': token, ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) throw Error((await response.json()).error);
  return response;
}
function display(next) {
  state = next;
  document.querySelector('#error').hidden = true;
  document.querySelector('#state').textContent = JSON.stringify(state, null, 2);
  document.querySelector('#status').textContent = `${state.fixture ? 'Generated fixture' : 'Workshop scene'} · ${state.width}×${state.height} · ${state.playing ? 'playing' : 'paused'}`;
}
function failure(error) {
  const panel = document.querySelector('#error');
  panel.hidden = false; panel.textContent = error.message;
  console.error('wallpaper-kit:', error.message);
}
function serial(work) {
  const result = pipeline.then(work);
  pipeline = result.catch(failure);
  return result;
}
async function draw() {
  const response = await request('/api/frame', {});
  const info = JSON.parse(response.headers.get('X-Wallpaper-Frame'));
  const bitmap = await createImageBitmap(await response.blob());
  canvas.width = info.width; canvas.height = info.height;
  context.drawImage(bitmap, 0, 0); bitmap.close();
  canvas.dataset.sha256 = info.sha256;
  canvas.dataset.timeMs = info.time_ms;
  display(await (await request('/api/state')).json());
  return structuredClone(state);
}
async function command(name, fields = {}) {
  const next = await (await request('/api/command', { name, ...fields })).json();
  display(next);
  return next;
}
function tick(now) {
  if (!playing) return;
  if (!framePending && now - lastPoll >= 1000 / 30) {
    lastPoll = now; framePending = true;
    serial(draw).catch(stopPolling).finally(() => { framePending = false; });
  }
  animationFrame = requestAnimationFrame(tick);
}
function updateActivity() {
  stopPolling();
  return serial(async () => {
    const run = wantsPlay && hostActive;
    const next = await command(run ? 'play' : 'pause');
    if (run && wantsPlay && hostActive && !playing) { playing = true; animationFrame = requestAnimationFrame(tick); }
    return next;
  });
}
const api = {
  inspect: () => serial(async () => { await command('inspect'); return structuredClone(state); }),
  capture: () => serial(draw),
  seek: time => { wantsPlay = false; stopPolling(); return serial(async () => { await command('seek', { time_ms: time }); return draw(); }); },
  resize: (width, height) => serial(async () => { await command('resize', { width, height }); return draw(); }),
  setProperties: properties => serial(async () => { await command('set_properties', { properties }); return draw(); }),
  setCalendarTime: unixMs => { wantsPlay = false; stopPolling(); return serial(async () => { await command('set_calendar_time', { unix_ms: unixMs }); return draw(); }); },
  play: () => { wantsPlay = true; return updateActivity(); },
  pause: () => { wantsPlay = false; return updateActivity(); },
  setActive: active => { hostActive = Boolean(active); return updateActivity(); },
  pixel: (x, y) => [...context.getImageData(x, y, 1, 1).data],
};
window.wallpaperTest = api;
api.ready = (async () => {
  const deadline = performance.now() + 60000;
  while (performance.now() < deadline) {
    const next = await (await request('/api/state')).json();
    if (next.error) throw Error(next.error);
    if (next.ready) { display(next); await serial(draw); return structuredClone(state); }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw Error('Native renderer did not become ready');
})();
api.ready.catch(failure);
for (const [button, work] of [
  ['play', api.play], ['pause', api.pause],
  ['seek', () => api.seek(Number(document.querySelector('#time').value))],
  ['resize', () => api.resize(Number(document.querySelector('#width').value), Number(document.querySelector('#height').value))],
]) document.querySelector(`#${button}`).addEventListener('click', () => { work().catch(() => {}); });
function activityChanged(active) { api.ready.then(() => api.setActive(active)).catch(failure); }
window.addEventListener('blur', () => activityChanged(false));
window.addEventListener('focus', () => activityChanged(!document.hidden));
document.addEventListener('visibilitychange', () => activityChanged(!document.hidden && document.hasFocus()));
