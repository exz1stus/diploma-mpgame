import { spawn } from 'node:child_process';
import { createReadStream, watch } from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { port: { type: 'string', default: '4173' }, 'api-port': { type: 'string', default: '4174' } } });
const frontendPort = Number(values.port);
const backendPort = Number(values['api-port']);
if (![frontendPort, backendPort].every(port => Number.isInteger(port) && port > 0 && port <= 65535) || frontendPort === backendPort) {
  throw new Error('Choose two distinct ports between 1 and 65535');
}

const root = resolve(import.meta.dirname, '../..');
const source = resolve(root, 'crates/worldgen/src');
const example = resolve(root, 'crates/worldgen/examples');
const manifest = resolve(root, 'crates/worldgen/Cargo.toml');
const dist = resolve(import.meta.dirname, 'dist');
const binary = resolve(root, 'target/debug/examples/worldgen_preview');
let generator = null;
let build = null;
let queued = false;
let timer = null;

const mime = { '.html': 'text/html', '.css': 'text/css', '.mjs': 'text/javascript' };
const staticFiles = new Set(['/', '/index.html', '/style.css', '/app.mjs', '/zoom.mjs', '/decode-worker.mjs', '/protocol.mjs', '/settings.mjs', '/biomes.mjs']);

const frontend = createServer((req, res) => {
  const path = new URL(req.url, 'http://localhost').pathname;
  if (path.startsWith('/api/')) {
    const proxy = httpRequest({ hostname: '127.0.0.1', port: backendPort, path: req.url, method: req.method }, upstream => {
      res.writeHead(upstream.statusCode, { ...upstream.headers, 'cache-control': 'no-store' });
      upstream.pipe(res);
    });
    proxy.on('error', () => {
      if (!res.headersSent) res.writeHead(503, { 'content-type': 'text/plain', 'cache-control': 'no-store' });
      res.end('Generator is rebuilding');
    });
    req.pipe(proxy);
    return;
  }
  if (!staticFiles.has(path)) { res.writeHead(404); res.end(); return; }
  const name = path === '/' ? 'index.html' : path.slice(1);
  const file = createReadStream(resolve(dist, name));
  file.on('error', () => { if (!res.headersSent) res.writeHead(404); res.end(); });
  file.on('open', () => {
    res.writeHead(200, { 'content-type': `${mime[name.slice(name.lastIndexOf('.'))]}; charset=utf-8`, 'cache-control': 'no-store' });
    file.pipe(res);
  });
});

function compile() {
  if (build) { queued = true; return; }
  console.log('\nBuilding the current Rust world generator…');
  build = spawn('cargo', ['build', '--offline', '-p', 'worldgen', '--example', 'worldgen_preview'], { cwd: root, stdio: 'inherit' });
  build.on('exit', code => {
    build = null;
    if (queued) { queued = false; compile(); return; }
    if (code !== 0) { console.error('Build failed. The previous generator remains available.'); return; }
    const start = () => {
      generator = spawn(binary, [], { cwd: root, stdio: 'inherit', env: { ...process.env, WORLDGEN_PREVIEW_ADDR: `127.0.0.1:${backendPort}` } });
      generator.on('exit', () => { generator = null; });
    };
    if (generator) { generator.once('exit', start); generator.kill('SIGTERM'); }
    else start();
  });
}

function schedule() { clearTimeout(timer); timer = setTimeout(compile, 180); }
const watchers = [
  watch(source, (_, name) => { if (name?.endsWith('.rs')) schedule(); }),
  watch(resolve(source, 'features'), (_, name) => { if (name?.endsWith('.rs') || name?.endsWith('.ron')) schedule(); }),
  watch(resolve(source, 'biomes'), (_, name) => { if (name?.endsWith('.rs') || name?.endsWith('.ron')) schedule(); }),
  watch(resolve(source, 'biomes/maps'), (_, name) => { if (name?.endsWith('.ron')) schedule(); }),
  watch(example, (_, name) => { if (name === 'worldgen_preview.rs') schedule(); }),
  watch(manifest, schedule),
];
frontend.listen(frontendPort, '127.0.0.1', () => console.log(`Worldgen frontend: http://127.0.0.1:${frontendPort}`));
function stop() { watchers.forEach(watcher => watcher.close()); generator?.kill('SIGTERM'); build?.kill('SIGTERM'); frontend.close(); }
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
compile();
