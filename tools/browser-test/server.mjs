import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

export const root = fileURLToPath(new URL('../../', import.meta.url));
const maxPng = 40 * 1024 * 1024;
const error = (message, status = 400) => Object.assign(new Error(message), { status });

class Worker {
  state = { ready: false, playing: false };
  pending = new Map();
  transcript = [];
  sequence = 0;
  buffer = Buffer.alloc(0);
  packet = null;
  exited = false;
  stderr = '';

  constructor(options, output, executable, sha256) {
    const args = ['--output', path.join(output, 'worker')];
    if (options.software) args.push('--software');
    if (options.scene) args.push('--scene', path.resolve(options.scene), '--engine-assets', path.resolve(options.engineAssets));
    this.child = spawn(executable, args, {
      cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.identity = { worker_pid: this.child.pid, renderer_sha256: sha256 };
    this.state = { ...this.state, ...this.identity };
    this.exit = new Promise(resolve => this.child.once('close', (code, signal) => {
      this.exited = true;
      if (code || !this.stopping) this.fail(error(`Renderer exited (${code ?? signal}): ${this.stderr}`, 503));
      resolve({ code, signal });
    }));
    this.child.on('error', e => this.fail(e));
    this.child.stderr.on('data', data => { this.stderr = (this.stderr + data).slice(-16384); });
    this.child.stdout.on('data', data => {
      try { this.receive(data); } catch (e) { this.fail(e); this.child.kill(); }
    });
  }

  fail(e) {
    this.state = { ...this.state, ready: false, error: e.message };
    for (const request of this.pending.values()) { clearTimeout(request.timer); request.reject(e); }
    this.pending.clear();
  }

  receive(data) {
    this.buffer = Buffer.concat([this.buffer, data]);
    if (this.buffer.length > maxPng + 1024 * 1024) throw error('Renderer packet exceeded transport bound', 503);
    while (true) {
      if (!this.packet) {
        const end = this.buffer.indexOf(10);
        if (end < 0) {
          if (this.buffer.length > 1024 * 1024) throw error('Renderer header exceeded transport bound', 503);
          return;
        }
        this.packet = JSON.parse(this.buffer.subarray(0, end).toString());
        this.buffer = this.buffer.subarray(end + 1);
        if (!Number.isSafeInteger(this.packet.bytes) || this.packet.bytes < 0 || this.packet.bytes > maxPng) throw error('Invalid renderer payload size', 503);
      }
      if (this.buffer.length < this.packet.bytes) return;
      const packet = this.packet;
      const png = this.buffer.subarray(0, packet.bytes);
      this.buffer = this.buffer.subarray(packet.bytes);
      this.packet = null;
      this.transcript.push(packet);
      if (this.transcript.length > 128) this.transcript.shift();
      if (packet.state) this.state = { ...packet.state, ...this.identity };
      if (packet.event === 'failed') { this.fail(error(packet.error, 503)); continue; }
      const request = this.pending.get(packet.id);
      if (request) {
        this.pending.delete(packet.id);
        clearTimeout(request.timer);
        if (packet.ok) request.resolve({ state: this.state, png });
        else request.reject(error(packet.error));
      }
      if (!this.buffer.length) return;
    }
  }

  request(command) {
    if (this.exited || this.state.error) return Promise.reject(error(this.state.error || 'Renderer is stopped', 503));
    if (!this.state.ready) return Promise.reject(error('Renderer is loading', 503));
    if (this.pending.size >= 4) return Promise.reject(error('Renderer command queue is full', 429));
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.fail(error('Renderer command timed out', 504)); this.child.kill(); }, 30000);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ id, command }) + '\n', e => { if (e) this.fail(e); });
    });
  }

  async close() {
    this.stopping = true;
    const timer = setTimeout(() => { if (!this.exited) this.child.kill(); }, 3000);
    try {
      if (!this.exited) {
        if (this.state.ready) await this.request({ name: 'stop' }).catch(() => {});
        else this.child.kill();
      }
      return await this.exit;
    } finally { clearTimeout(timer); }
  }
}

function body(request) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    request.on('data', chunk => {
      size += chunk.length;
      if (size <= 8192) chunks.push(chunk);
      else reject(error('Command body exceeds 8 KiB', 413));
    });
    request.on('end', () => {
      try { resolve(JSON.parse(Buffer.concat(chunks).toString())); } catch { reject(error('Invalid JSON command')); }
    });
    request.on('error', reject);
  });
}

export async function startHarness(options = {}) {
  if (options.scene && !options.engineAssets) throw error('A scene requires engineAssets');
  const output = path.resolve(options.output || path.join(root, 'artifacts', `browser-${Date.now()}-${randomBytes(3).toString('hex')}`));
  await mkdir(path.dirname(output), { recursive: true });
  await mkdir(output);
  // Pin the build so a connected browser does not lock Cargo's next executable.
  const executable = path.join(output, 'browser-worker.exe');
  await copyFile(path.join(root, 'target/debug/examples/browser-worker.exe'), executable);
  const rendererSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
  const token = randomBytes(24).toString('hex');
  const assets = new Map(await Promise.all([
    ['/', 'index.html', 'text/html'], ['/app.js', 'app.js', 'text/javascript'], ['/style.css', 'style.css', 'text/css'],
  ].map(async ([route, file, type]) => [route, { data: await readFile(new URL(`web/${file}`, import.meta.url)), type }])));
  let origin;
  let worker;
  let frameInFlight = false;
  const server = http.createServer({ maxHeaderSize: 16384 }, async (request, response) => {
    response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('Content-Security-Policy', "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' blob:; base-uri 'none'; frame-ancestors 'none'");
    const json = (value, status = 200) => { response.writeHead(status, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(value)); };
    try {
      if (`http://${request.headers.host}` !== origin || (request.headers.origin && request.headers.origin !== origin)) throw error('Origin is not allowed', 403);
      const route = new URL(request.url, origin).pathname;
      if (request.method === 'GET' && route === '/favicon.ico') { response.writeHead(204); response.end(); return; }
      if (request.method === 'GET' && assets.has(route)) {
        const asset = assets.get(route);
        response.writeHead(200, { 'Content-Type': `${asset.type}; charset=utf-8` }); response.end(asset.data); return;
      }
      if (request.headers['x-wallpaper-token'] !== token) throw error('Invalid session token', 403);
      if (request.method === 'GET' && route === '/api/state') { json(worker.state); return; }
      if (request.method === 'POST' && route === '/api/command') {
        const command = await body(request);
        if (!['inspect', 'play', 'pause', 'seek', 'resize'].includes(command?.name)) throw error('Unknown browser command');
        json((await worker.request(command)).state); return;
      }
      if (request.method === 'POST' && route === '/api/frame') {
        if (frameInFlight) throw error('A frame is already in flight', 429);
        frameInFlight = true;
        try {
          const { state, png } = await worker.request({ name: 'frame' });
          response.writeHead(200, { 'Content-Type': 'image/png', 'X-Wallpaper-Frame': JSON.stringify(state.frame) });
          await new Promise(resolve => { response.once('close', resolve); response.end(png, resolve); });
        } finally { frameInFlight = false; }
        return;
      }
      throw error('Route not found', 404);
    } catch (e) { json({ error: e.message }, e.status || 500); }
  });
  server.requestTimeout = 35000;
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(options.port || 0, '127.0.0.1', resolve); });
  origin = `http://127.0.0.1:${server.address().port}`;
  worker = new Worker(options, output, executable, rendererSha256);
  const url = `${origin}/#${token}`;
  await writeFile(path.join(output, 'server.json'), JSON.stringify({ url, worker_pid: worker.child.pid, renderer_sha256: rendererSha256, executable, output }, null, 2));
  let closing;
  return {
    url, origin, output, token, workerPid: worker.child.pid,
    close() {
      return closing ||= (async () => {
        const closed = new Promise(resolve => server.close(resolve));
        server.closeAllConnections();
        const exit = await worker.close();
        await closed;
        await writeFile(path.join(output, 'renderer-transcript.json'), JSON.stringify(worker.transcript, null, 2));
        await writeFile(path.join(output, 'cleanup.json'), JSON.stringify({ worker_pid: worker.child.pid, ...exit, server_closed: true }, null, 2));
        return exit;
      })();
    },
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: {
    scene: { type: 'string' }, 'engine-assets': { type: 'string' }, software: { type: 'boolean' },
    port: { type: 'string' }, output: { type: 'string' },
  } });
  const port = values.port === undefined ? 0 : Number(values.port);
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw error('Invalid port');
  const harness = await startHarness({ scene: values.scene, engineAssets: values['engine-assets'], software: values.software, port, output: values.output });
  console.log(JSON.stringify({ event: 'browser_test_ready', url: harness.url, output: harness.output }));
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => { harness.close().catch(e => { console.error(e); process.exitCode = 1; }); });
}
