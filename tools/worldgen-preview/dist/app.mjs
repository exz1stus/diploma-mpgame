import { zoomMetrics, fitWorldZoom, clampCamera } from './zoom.mjs';
import { SAMPLE_BYTES, readSample } from './protocol.mjs';
import { defaultSettings, settingsError, readSettings, settingsQuery } from './settings.mjs';

const canvas = document.querySelector('#map');
const viewport = document.querySelector('.viewport');
const ctx = canvas.getContext('2d', { alpha: false });
let controls = [];
let inputs = {};
const settingsPanel = document.querySelector('#generator-settings');
const labels = {
  coords: document.querySelector('#coords'),
  readout: document.querySelector('#readout'),
  status: document.querySelector('#status'),
  zoom: document.querySelector('#zoom-level'),
  validation: document.querySelector('#validation'),
};
const tooltip = document.querySelector('#tile-tooltip');
const tooltipFields = {
  tile: document.querySelector('#tooltip-tile'),
  temperature: document.querySelector('#tooltip-temperature'),
  height: document.querySelector('#tooltip-height'),
  note: document.querySelector('#tooltip-note'),
};
let world = null;
let metaRequest = 0;
const params = new URLSearchParams(location.search);
const apiBase = (params.get('api') || '').replace(/\/$/, '');
const initialSize = Number(params.get('chunk') ?? 32);
let config = { size: Number.isInteger(initialSize) && initialSize >= 4 && initialSize <= 128 ? initialSize : 32 };
let zoom = 0;
let cameraX = 0;
let cameraY = 0;
let pointer = null;
let dragging = null;
let frame = 0;
let cache = new Map();
let staleCache = new Map();
let revision = 0;
let buildId = null;
const active = new Map();
const decoders = Array.from({ length: Math.min(4, Math.max(2, navigator.hardwareConcurrency || 2)) }, () => new Worker('./decode-worker.mjs', { type: 'module' }));
const decoderJobs = new Map();
let decoderCursor = 0;
let decoderId = 0;
for (const worker of decoders) worker.onmessage = ({ data }) => {
  const job = decoderJobs.get(data.id);
  if (!job) { data.image?.close(); return; }
  decoderJobs.delete(data.id);
  if (data.error) job.reject(new Error(data.error));
  else job.resolve({ bytes: new Uint8Array(data.buffer), image: data.image });
};

function decode(buffer, size) {
  return new Promise((resolve, reject) => {
    const id = ++decoderId;
    decoderJobs.set(id, { resolve, reject });
    decoders[decoderCursor++ % decoders.length].postMessage({ id, buffer, size }, [buffer]);
  });
}
let retryAt = 0;
let inputError = '';

function installControls(nextControls) {
  const firstLoad = controls.length === 0;
  const previous = config;
  controls = nextControls;
  config = firstLoad ? readSettings(params, controls) : { ...defaultSettings(controls), ...previous };
  inputs = {};
  settingsPanel.replaceChildren();
  for (const group of new Set(controls.map(control => control.group))) {
    const section = document.createElement('details');
    section.className = 'settings-group';
    section.open = true;
    const title = document.createElement('summary');
    title.textContent = group;
    section.append(title);
    for (const control of controls.filter(control => control.group === group)) {
      const label = document.createElement('label');
      label.className = 'field';
      const text = document.createElement('span');
      text.textContent = control.label;
      let input;
      if (control.key === 'instant') {
        input = document.createElement('select');
        for (const [value, name] of [[0, 'Daily average'], [1, 'At time of day']]) {
          const option = document.createElement('option');
          option.value = value; option.textContent = name; input.append(option);
        }
      } else {
        input = document.createElement('input');
        input.type = 'number';
        input.min = control.min; input.max = control.max; input.step = control.step;
        input.required = control.default !== null;
        if (control.default === null) input.placeholder = 'World seed';
      }
      input.id = control.key;
      input.addEventListener('change', applyInputs);
      inputs[control.key] = input;
      label.append(text, input); section.append(label);
    }
    const hints = {
      World: 'Terrain coordinate scale multiplies elevation noise frequency. Regional noise uses global tile coordinates.',
      'Elevation noise': 'Leave seed override blank to follow the world seed.',
      'Temperature region noise': 'Leave seed override blank to derive it from the world seed.',
      'Temperature coefficients': 'Daily variation affects the time-of-day display. All coefficients use Celsius.',
      'Planet and time': 'Orbit: 0 / 0.5 equinox, 0.25 northern summer, 0.75 northern winter. Time: 0 midnight, 0.5 noon at the prime meridian.',
    };
    const hint = document.createElement('p');
    hint.className = 'settings-hint'; hint.textContent = hints[group] || '';
    section.append(hint); settingsPanel.append(section);
  }
  fillInputs();
  document.querySelector('#randomize').disabled = false;
  document.querySelector('#reset-settings').disabled = false;
}

function fillInputs() {
  for (const [key, input] of Object.entries(inputs)) input.value = config[key] ?? '';
  if (inputs.time) inputs.time.disabled = config.instant === 0;
}

function updateUrl() {
  const query = settingsQuery(config, controls, true);
  if (apiBase) query.set('api', apiBase);
  history.replaceState(null, '', `${location.pathname}?${query}`);
}

function applyInputs() {
  const candidate = Object.fromEntries(controls.map(control => {
    const raw = inputs[control.key].value.trim();
    return [control.key, raw === '' ? (control.default === null ? null : NaN) : Number(raw)];
  }));
  const error = settingsError(candidate, controls);
  inputError = error;
  if (error) {
    labels.validation.textContent = error;
    return;
  }
  labels.validation.textContent = '';
  const chunkSizeChanged = candidate.size !== config.size;
  config = candidate;
  inputs.time.disabled = config.instant === 0;
  clearCache(staleCache);
  if (chunkSizeChanged) { clearCache(cache); world = null; }
  staleCache = cache;
  cache = new Map();
  revision++;
  for (const controller of active.values()) controller.abort();
  active.clear();
  updateUrl();
  pollBuild();
  invalidate();
}

function metrics() {
  return zoomMetrics(zoom);
}

function zoomAt(amount, screenX, screenY) {
  const old = metrics();
  const oldX = cameraX + (screenX - viewport.clientWidth / 2) / old.cellPx;
  const oldY = cameraY + (screenY - viewport.clientHeight / 2) / old.cellPx;
  if (!world) return;
  zoom = Math.min(fitWorldZoom(world, viewport.clientWidth, viewport.clientHeight), Math.max(-6, zoom + amount));
  const next = metrics();
  const factor = 2 ** (old.lod - next.lod);
  cameraX = oldX * factor - (screenX - viewport.clientWidth / 2) / next.cellPx;
  cameraY = oldY * factor - (screenY - viewport.clientHeight / 2) / next.cellPx;
  invalidate();
}

function constrainView() {
  if (!world) return;
  const previous = metrics();
  zoom = Math.min(zoom, fitWorldZoom(world, viewport.clientWidth, viewport.clientHeight));
  const next = metrics();
  const factor = 2 ** (previous.lod - next.lod);
  const camera = clampCamera(cameraX * factor, cameraY * factor, next, viewport.clientWidth, viewport.clientHeight, world);
  cameraX = camera.x;
  cameraY = camera.y;
}

function cellAt(screenX, screenY, m = metrics()) {
  return {
    x: Math.floor(cameraX + (screenX - viewport.clientWidth / 2) / m.cellPx),
    y: Math.floor(cameraY + (screenY - viewport.clientHeight / 2) / m.cellPx),
  };
}

function key(cx, cy, lod) { return `${lod}:${cx}:${cy}`; }

function clearCache(map) {
  for (const chunk of map.values()) chunk.image.close();
  map.clear();
}

function fetchVisible(minX, minY, maxX, maxY, lod) {
  if (Date.now() < retryAt || lod > 60 || ![minX, minY, maxX, maxY].every(Number.isSafeInteger)) return;
  const wanted = [];
  const visible = new Set();
  const centerX = (minX + maxX) / 2;
  const centerY = (minY + maxY) / 2;
  for (let cy = minY; cy <= maxY; cy++) for (let cx = minX; cx <= maxX; cx++) {
    const id = key(cx, cy, lod);
    visible.add(id);
    if (!cache.has(id) && !active.has(id)) wanted.push({ cx, cy, id, distance: (cx - centerX) ** 2 + (cy - centerY) ** 2 });
  }
  for (const [id, controller] of active) if (!visible.has(id)) { controller.abort(); active.delete(id); }
  wanted.sort((a, b) => a.distance - b.distance);
  for (const { cx, cy, id } of wanted.slice(0, Math.max(0, 6 - active.size))) {
    const controller = new AbortController();
    active.set(id, controller);
    const requestRevision = revision;
    const size = config.size;
    const query = settingsQuery(config, controls);
    for (const [key, value] of Object.entries({ min_x: cx, min_y: cy, max_x: cx, max_y: cy, lod })) query.set(key, value);
    (async () => {
      try {
        const response = await fetch(`${apiBase}/api/region?${query}`, { signal: controller.signal });
        if (!response.ok) throw new Error(`Generator returned ${response.status}`);
        const buffer = await response.arrayBuffer();
        if (buffer.byteLength !== size * size * SAMPLE_BYTES) throw new Error('Incomplete chunk data');
        if (requestRevision !== revision || controller.signal.aborted) return;
        const chunk = await decode(buffer, size);
        if (requestRevision !== revision || controller.signal.aborted) { chunk.image.close(); return; }
        cache.set(id, chunk);
        while (cache.size > Math.min(2000, Math.floor(64 * 1024 ** 2 / (size * size * (SAMPLE_BYTES + 4))))) {
          const oldest = cache.keys().next().value;
          cache.get(oldest).image.close(); cache.delete(oldest);
        }
        labels.validation.textContent = inputError;
      } catch (error) {
        if (error.name !== 'AbortError') {
          retryAt = Date.now() + 500;
          labels.validation.textContent = 'Generator is rebuilding or unavailable. Retrying…';
          setTimeout(invalidate, 500);
        }
      } finally {
        if (active.get(id) === controller) active.delete(id);
        invalidate();
      }
    })();
  }
}

function drawFallback(cx, cy, lod, edgeX, edgeY, span) {
  const current = cache.get(key(cx, cy, lod)) || staleCache.get(key(cx, cy, lod));
  if (current) { ctx.drawImage(current.image, edgeX, edgeY, span, span); return; }
  // A neighboring LOD keeps the map visible while exact chunks stream in.
  for (const source of [cache, staleCache]) {
    const coarser = source.get(key(Math.floor(cx / 2), Math.floor(cy / 2), lod + 1));
    if (coarser) {
      ctx.save();
      ctx.beginPath(); ctx.rect(edgeX, edgeY, span, span); ctx.clip();
      ctx.drawImage(coarser.image, edgeX - (cx & 1) * span, edgeY - (cy & 1) * span, span * 2, span * 2);
      ctx.restore();
      return;
    }
    if (lod > 0) {
      let found = false;
      for (let dy = 0; dy < 2; dy++) for (let dx = 0; dx < 2; dx++) {
        const finer = source.get(key(cx * 2 + dx, cy * 2 + dy, lod - 1));
        if (finer) {
          ctx.drawImage(finer.image, edgeX + dx * span / 2, edgeY + dy * span / 2, span / 2, span / 2);
          found = true;
        }
      }
      if (found) return;
    }
  }
}

function draw() {
  frame = 0;
  const width = viewport.clientWidth;
  const height = viewport.clientHeight;
  if (!width || !height) return;
  const dpr = Math.min(devicePixelRatio || 1, 2);
  const targetWidth = Math.round(width * dpr);
  const targetHeight = Math.round(height * dpr);
  if (canvas.width !== targetWidth || canvas.height !== targetHeight) {
    canvas.width = targetWidth;
    canvas.height = targetHeight;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.imageSmoothingEnabled = false;
  ctx.fillStyle = '#0b1720';
  ctx.fillRect(0, 0, width, height);
  if (!world) { labels.status.textContent = 'Waiting for world dimensions…'; tooltip.hidden = true; return; }
  constrainView();
  const m = metrics();
  const stride = 2 ** m.lod;
  const worldX = width / 2 - cameraX * m.cellPx;
  const worldY = height / 2 - cameraY * m.cellPx;
  ctx.save();
  ctx.beginPath();
  ctx.rect(worldX, worldY, world.width * m.baseTilePx, world.height * m.baseTilePx);
  ctx.clip();
  ctx.fillStyle = '#142630';
  ctx.fillRect(0, 0, width, height);
  const left = Math.max(0, Math.floor(cameraX - width / (2 * m.cellPx)) - 1);
  const top = Math.max(0, Math.floor(cameraY - height / (2 * m.cellPx)) - 1);
  const right = Math.min(Math.ceil(world.width / stride), Math.ceil(cameraX + width / (2 * m.cellPx)) + 1);
  const bottom = Math.min(Math.ceil(world.height / stride), Math.ceil(cameraY + height / (2 * m.cellPx)) + 1);
  const chunkSize = config.size;
  const minChunkX = Math.floor(left / chunkSize);
  const minChunkY = Math.floor(top / chunkSize);
  const maxChunkX = Math.floor((right - 1) / chunkSize);
  const maxChunkY = Math.floor((bottom - 1) / chunkSize);
  for (let cy = minChunkY; cy <= maxChunkY; cy++) {
    for (let cx = minChunkX; cx <= maxChunkX; cx++) {
      const edgeX = width / 2 + (cx * chunkSize - cameraX) * m.cellPx;
      const edgeY = height / 2 + (cy * chunkSize - cameraY) * m.cellPx;
      const span = chunkSize * m.cellPx;
      drawFallback(cx, cy, m.lod, edgeX, edgeY, span);
      ctx.strokeStyle = '#d3f3e477';
      ctx.lineWidth = 1.5;
      ctx.strokeRect(Math.round(edgeX) + .5, Math.round(edgeY) + .5, span, span);
      if (span > 120 && edgeX < width && edgeY < height && edgeX + span > 0 && edgeY + span > 0) {
        ctx.fillStyle = '#09202bd9';
        ctx.fillRect(edgeX + 1, edgeY + 1, 105, 22);
        ctx.fillStyle = '#c3e9dd';
        ctx.font = '10px ui-monospace, monospace';
        ctx.fillText(`CHUNK ${cx}, ${cy}`, edgeX + 8, edgeY + 16);
      }
    }
  }
  fetchVisible(minChunkX, minChunkY, maxChunkX, maxChunkY, m.lod);
  if (m.cellPx >= 18) {
    ctx.strokeStyle = '#07172022';
    ctx.lineWidth = 1;
    for (let x = left; x <= right; x++) {
      const px = Math.round(width / 2 + (x - cameraX) * m.cellPx) + .5;
      ctx.beginPath(); ctx.moveTo(px, 0); ctx.lineTo(px, height); ctx.stroke();
    }
    for (let y = top; y <= bottom; y++) {
      const py = Math.round(height / 2 + (y - cameraY) * m.cellPx) + .5;
      ctx.beginPath(); ctx.moveTo(0, py); ctx.lineTo(width, py); ctx.stroke();
    }
  }
  ctx.restore();
  labels.zoom.textContent = zoom >= 0 ? `2^−${zoom.toFixed(1)}×` : `${2 ** -zoom < 1e6 ? (2 ** -zoom).toFixed(1) : `2^${(-zoom).toFixed(0)}`}×`;
  labels.status.textContent = m.lod === 0 ? `LOD 0 · exact tiles · ${m.baseTilePx.toFixed(2)} px / tile` : `LOD ${m.lod} · 2^${m.lod} tiles / cell`;

  updateReadout();
}

function updateReadout() {
  tooltip.hidden = true;
  if (!world) return;
  const m = metrics();
  const stride = 2 ** m.lod;
  labels.coords.textContent = `TILE ${Math.floor(cameraX * stride)}, ${Math.floor(cameraY * stride)} / WORLD ${world.width} × ${world.height}`;
  if (!pointer || dragging) {
    labels.readout.textContent = 'Hover a tile to inspect temperature and height';
    return;
  }
  const worldX = (cameraX + (pointer.x - viewport.clientWidth / 2) / m.cellPx) * stride;
  const worldY = (cameraY + (pointer.y - viewport.clientHeight / 2) / m.cellPx) * stride;
  if (worldX < 0 || worldX >= world.width || worldY < 0 || worldY >= world.height) return;
  const cell = cellAt(pointer.x, pointer.y, m);
  const chunkX = Math.floor(cell.x / config.size);
  const chunkY = Math.floor(cell.y / config.size);
  const chunk = cache.get(key(chunkX, chunkY, m.lod));
  const index = (cell.y - chunkY * config.size) * config.size + cell.x - chunkX * config.size;
  const sample = chunk ? readSample(chunk.bytes, index) : null;
  const offset = m.lod === 0 ? 0 : stride / 2;
  const tileX = Math.min(world.width - 1, cell.x * stride + offset);
  const tileY = Math.min(world.height - 1, cell.y * stride + offset);
  const title = sample ? (sample.status === 1 ? sample.color : 'No matching biome') : 'Loading tile…';
  const temperature = sample && Number.isFinite(sample.temperature) ? `${sample.temperature.toFixed(1)} °C` : '—';
  const height = sample && Number.isFinite(sample.height) ? sample.height.toFixed(4) : '—';
  labels.readout.textContent = `${title} · ${temperature} · height ${height}`;
  tooltipFields.tile.textContent = `${m.lod === 0 ? 'Tile' : 'Sample'} ${tileX}, ${tileY} · ${title}`;
  tooltipFields.temperature.textContent = temperature;
  tooltipFields.height.textContent = height;
  tooltipFields.note.textContent = `Height 0–1 · ${config.instant ? `temperature at UTC ${(config.time * 24).toFixed(1)}h` : 'daily average temperature'}${m.lod ? ` · LOD ${m.lod}` : ''}`;
  tooltip.hidden = false;
  const tooltipWidth = tooltip.offsetWidth;
  const tooltipHeight = tooltip.offsetHeight;
  const x = pointer.x + 16 + tooltipWidth > viewport.clientWidth ? pointer.x - tooltipWidth - 16 : pointer.x + 16;
  const y = pointer.y + 16 + tooltipHeight > viewport.clientHeight ? pointer.y - tooltipHeight - 16 : pointer.y + 16;
  tooltip.style.left = `${Math.max(8, Math.min(x, viewport.clientWidth - tooltipWidth - 8))}px`;
  tooltip.style.top = `${Math.max(8, Math.min(y, viewport.clientHeight - tooltipHeight - 8))}px`;
}

function invalidate() { if (!frame) frame = requestAnimationFrame(draw); }

document.querySelector('#randomize').addEventListener('click', () => {
  inputs.seed.value = crypto.getRandomValues(new Uint32Array(1))[0];
  applyInputs();
});
document.querySelector('#reset-settings').addEventListener('click', () => {
  for (const control of controls) inputs[control.key].value = control.default ?? '';
  applyInputs();
});
document.querySelector('#reset').addEventListener('click', () => {
  zoom = 0; cameraX = world ? world.width / 2 : 0; cameraY = world ? world.height / 2 : 0; invalidate();
});
document.querySelector('#fit-world').addEventListener('click', () => {
  if (!world) return;
  zoom = fitWorldZoom(world, viewport.clientWidth, viewport.clientHeight);
  const stride = 2 ** metrics().lod;
  cameraX = world.width / (2 * stride); cameraY = world.height / (2 * stride);
  invalidate();
});
document.querySelector('#zoom-in').addEventListener('click', () => zoomAt(-1, viewport.clientWidth / 2, viewport.clientHeight / 2));
document.querySelector('#zoom-out').addEventListener('click', () => zoomAt(1, viewport.clientWidth / 2, viewport.clientHeight / 2));
viewport.addEventListener('wheel', event => {
  event.preventDefault();
  const rect = viewport.getBoundingClientRect();
  zoomAt(Math.sign(event.deltaY) * .25, event.clientX - rect.left, event.clientY - rect.top);
}, { passive: false });
viewport.addEventListener('pointerdown', event => {
  if (event.target.closest('button')) return;
  viewport.setPointerCapture(event.pointerId);
  dragging = { x: event.clientX, y: event.clientY };
  viewport.classList.add('dragging');
});
viewport.addEventListener('pointermove', event => {
  if (event.target.closest('button, .zoom-tools')) { pointer = null; invalidate(); return; }
  const rect = viewport.getBoundingClientRect();
  pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top };
  if (dragging) {
    const m = metrics();
    cameraX -= (event.clientX - dragging.x) / m.cellPx;
    cameraY -= (event.clientY - dragging.y) / m.cellPx;
    dragging = { x: event.clientX, y: event.clientY };
  }
  invalidate();
});
viewport.addEventListener('pointerup', () => { dragging = null; viewport.classList.remove('dragging'); invalidate(); });
viewport.addEventListener('pointercancel', () => { dragging = null; viewport.classList.remove('dragging'); invalidate(); });
viewport.addEventListener('pointerleave', () => { if (!dragging) pointer = null; invalidate(); });
new ResizeObserver(invalidate).observe(viewport);
async function pollBuild() {
  const request = ++metaRequest;
  try {
    const response = await fetch(`${apiBase}/api/meta?size=${config.size}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('Preview server unavailable');
    const { build, protocol, world: nextWorld, controls: nextControls } = await response.json();
    if (request !== metaRequest) return;
    if (protocol !== 2 || !nextWorld || !Number.isSafeInteger(nextWorld.width) || !Number.isSafeInteger(nextWorld.height)
      || nextWorld.width <= 0 || nextWorld.height <= 0 || !Array.isArray(nextControls) || !nextControls.length) throw new Error('Unsupported preview metadata');
    if (JSON.stringify(controls) !== JSON.stringify(nextControls)) installControls(nextControls);
    if (buildId !== null && build !== buildId) {
      revision++;
      clearCache(staleCache);
      staleCache = cache;
      cache = new Map();
      for (const controller of active.values()) controller.abort();
      active.clear();
    }
    if (!world) {
      const stride = 2 ** metrics().lod;
      cameraX = nextWorld.width / (2 * stride);
      cameraY = nextWorld.height / (2 * stride);
    }
    world = nextWorld;
    buildId = build;
    labels.validation.textContent = inputError;
    invalidate();
  } catch {
    if (request === metaRequest) labels.validation.textContent = 'Generator is rebuilding or unavailable. Waiting for the local server…';
  }
}
setInterval(pollBuild, 1200);
pollBuild();
invalidate();
