import { spawn } from 'node:child_process';
import { createReadStream, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync, watch } from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import { dirname, resolve } from 'node:path';
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
const config = resolve(root, 'crates/worldgen/config');
// The client's presentation files: read by the frontend editor, never written by it.
const clientAssets = resolve(root, 'crates/client/assets');
// Exported presentation files, laid out like crates/client/assets.
const exportDir = resolve(import.meta.dirname, 'export/assets');
const dist = resolve(import.meta.dirname, 'dist');
const binary = resolve(root, 'target/debug/examples/worldgen_preview');
let generator = null;
let build = null;
let queued = false;
let timer = null;

const mime = { '.html': 'text/html', '.css': 'text/css', '.mjs': 'text/javascript', '.png': 'image/png' };
const staticFiles = new Set(['/', '/index.html', '/style.css', '/app.mjs', '/zoom.mjs', '/decode-worker.mjs', '/protocol.mjs', '/settings.mjs', '/biomes.mjs', '/objects.mjs', '/terrain.mjs', '/presentation.mjs', '/editor.mjs']);
// The sprite sheet lives next to the preview example so artists edit a single source image.
const assets = { '/textures.png': resolve(example, 'textures.png') };

const textureName = /^[A-Za-z0-9_-]+$/;
const exportPath = /^(world\/(tiles|objects)\.ron|textures\/(tiles|objects)\/[A-Za-z0-9_-]+\.png)$/;

// Client presentation files: /client-assets/index.json lists texture files,
// /client-assets/world/{tiles,objects}.ron and /client-assets/textures/<folder>/<name>.png.
function clientAsset(path) {
  if (path === '/client-assets/index.json') {
    const list = folder => {
      try { return readdirSync(resolve(clientAssets, 'textures', folder)).filter(name => name.endsWith('.png')).map(name => name.slice(0, -4)).filter(name => textureName.test(name)); } catch { return []; }
    };
    return { json: { tiles: list('tiles'), objects: list('objects') } };
  }
  const world = /^\/client-assets\/world\/(presets\/old\/)?(tiles|objects)\.ron$/.exec(path);
  if (world) return { file: resolve(clientAssets, 'world', world[1] ?? '', `${world[2]}.ron`), type: 'text/plain' };
  const texture = /^\/client-assets\/textures\/(tiles|objects)\/([A-Za-z0-9_-]+)\.png$/.exec(path);
  if (texture) return { file: resolve(clientAssets, 'textures', texture[1], `${texture[2]}.png`), type: 'image/png' };
  return null;
}

// Replaces export/assets with the posted files: { files: [{ path, text } | { path, base64 }] }.
function exportFiles(req, res) {
  let body = '';
  req.setEncoding('utf8');
  req.on('data', chunk => { body += chunk; if (body.length > 64 * 1024 ** 2) req.destroy(); });
  req.on('end', () => {
    try {
      const { files } = JSON.parse(body);
      if (!Array.isArray(files) || !files.length || files.some(file => !exportPath.test(file?.path ?? ''))) throw new Error('Invalid export file list');
      rmSync(exportDir, { recursive: true, force: true });
      for (const file of files) {
        const target = resolve(exportDir, file.path);
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, typeof file.base64 === 'string' ? Buffer.from(file.base64, 'base64') : String(file.text ?? ''));
      }
      console.log(`Exported ${files.length} presentation files to ${exportDir}`);
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ directory: exportDir, files: files.map(file => file.path) }));
    } catch (error) {
      res.writeHead(400, { 'content-type': 'text/plain' });
      res.end(error.message);
    }
  });
}

const frontend = createServer((req, res) => {
  const path = new URL(req.url, 'http://localhost').pathname;
  if (path === '/export' && req.method === 'POST') { exportFiles(req, res); return; }
  const asset = path.startsWith('/client-assets/') ? clientAsset(path) : null;
  if (asset?.json) {
    res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
    res.end(JSON.stringify(asset.json));
    return;
  }
  if (asset?.file) {
    if (!existsSync(asset.file)) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': asset.type, 'cache-control': 'no-store' });
    createReadStream(asset.file).pipe(res);
    return;
  }
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
  if (!staticFiles.has(path) && !Object.hasOwn(assets, path)) { res.writeHead(404); res.end(); return; }
  const name = path === '/' ? 'index.html' : path.slice(1);
  const file = createReadStream(assets[path] ?? resolve(dist, name));
  file.on('error', () => { if (!res.headersSent) res.writeHead(404); res.end(); });
  file.on('open', () => {
    const type = mime[name.slice(name.lastIndexOf('.'))];
    res.writeHead(200, { 'content-type': type.startsWith('image/') ? type : `${type}; charset=utf-8`, 'cache-control': 'no-store' });
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
  watch(source, { recursive: true }, (_, name) => { if (name?.endsWith('.rs')) schedule(); }),
  watch(config, { recursive: true }, (_, name) => { if (name?.endsWith('.ron')) schedule(); }),
  watch(example, (_, name) => { if (name === 'worldgen_preview.rs') schedule(); }),
  watch(manifest, schedule),
];
frontend.listen(frontendPort, '127.0.0.1', () => console.log(`Worldgen frontend: http://127.0.0.1:${frontendPort}`));
function stop() { watchers.forEach(watcher => watcher.close()); generator?.kill('SIGTERM'); build?.kill('SIGTERM'); frontend.close(); }
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
compile();
