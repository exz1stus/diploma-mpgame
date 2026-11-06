import test from 'node:test';
import assert from 'node:assert/strict';
import { SAMPLE_BYTES, decodePixels, readSample } from './dist/protocol.mjs';
import { zoomMetrics, fitWorldZoom, clampCamera } from './dist/zoom.mjs';
import { defaultSettings, settingsError, readSettings, settingsQuery } from './dist/settings.mjs';

const controls = [
  { key: 'size', label: 'Chunk size', default: 32, min: 4, max: 128, step: '1' },
  { key: 'temperature_base', label: 'Base', default: -30, min: -1000, max: 1000, step: 'any' },
  { key: 'region_seed', label: 'Seed override', default: null, min: 0, max: 4294967295, step: '1' },
];

test('settings round-trip through shared URLs and API requests, including seed zero', () => {
  const config = { size: 64, temperature_base: -42.5, region_seed: 0 };
  const url = settingsQuery(config, controls, true);
  assert.equal(url.get('chunk'), '64');
  assert.deepEqual(readSettings(url, controls), config);
  const api = settingsQuery(config, controls);
  assert.equal(api.get('size'), '64');
  assert.equal(api.get('region_seed'), '0');
  assert.equal(api.get('temperature_base'), '-42.5');
});

test('optional seeds are omitted, and invalid URL values fall back to generator defaults', () => {
  const defaults = defaultSettings(controls);
  assert.equal(settingsQuery(defaults, controls).has('region_seed'), false);
  assert.deepEqual(readSettings(new URLSearchParams('chunk=1.5&temperature_base=NaN&region_seed=-1'), controls), defaults);
  assert.deepEqual(readSettings(new URLSearchParams('temperature_base=&region_seed='), controls), defaults);
  assert.equal(settingsError(defaults, controls), '');
  assert.match(settingsError({ ...defaults, size: 1.5 }, controls), /integer/);
  assert.match(settingsError({ ...defaults, temperature_base: NaN }, controls), /Base/);
});

test('generator colors and Celsius/height data survive binary decoding', () => {
  const buffer = new ArrayBuffer(4 * SAMPLE_BYTES);
  const bytes = new Uint8Array(buffer);
  const view = new DataView(buffer);
  for (let i = 0; i < 4; i++) {
    bytes.set([1, 56, 71, 106], i * SAMPLE_BYTES);
    view.setFloat32(i * SAMPLE_BYTES + 4, 0.625, true);
    view.setFloat32(i * SAMPLE_BYTES + 8, -12.5, true);
  }
  assert.deepEqual(readSample(bytes, 0), { status: 1, color: '#38476a', height: 0.625, temperature: -12.5 });
  assert.deepEqual([...decodePixels(buffer, 2).slice(0, 4)], [56, 71, 106, 255]);
});

test('unclassified cells stay visible and world padding stays transparent', () => {
  const buffer = new ArrayBuffer(4 * SAMPLE_BYTES);
  const bytes = new Uint8Array(buffer);
  bytes[SAMPLE_BYTES] = 255;
  const pixels = decodePixels(buffer, 2);
  assert.equal(pixels[3], 255);
  assert.equal(pixels[7], 0);
  assert.throws(() => decodePixels(new ArrayBuffer(5), 2), /Incomplete chunk data/);
});

test('maximum zoom out fits the full world for square and rectangular views', () => {
  for (const [world, width, height] of [
    [{ width: 32000, height: 32000 }, 1000, 800],
    [{ width: 64000, height: 32000 }, 800, 1000],
    [{ width: 4, height: 4 }, 1200, 800],
  ]) {
    const metrics = zoomMetrics(fitWorldZoom(world, width, height));
    assert.ok(world.width * metrics.baseTilePx <= width + 1e-9);
    assert.ok(world.height * metrics.baseTilePx <= height + 1e-9);
    assert.ok(Math.abs(world.width * metrics.baseTilePx - width) < 1e-9
      || Math.abs(world.height * metrics.baseTilePx - height) < 1e-9);
  }
});

test('panning is constrained at every world edge', () => {
  const world = { width: 1000, height: 800 };
  const metrics = zoomMetrics(0);
  assert.deepEqual(clampCamera(-1000, -1000, metrics, 480, 480, world), { x: 10, y: 10 });
  assert.deepEqual(clampCamera(1e9, 1e9, metrics, 480, 480, world), { x: 990, y: 790 });
});

test('fit view stays centered across LOD changes and viewport resizing', () => {
  const world = { width: 32000, height: 16000 };
  const metrics = zoomMetrics(fitWorldZoom(world, 1000, 800));
  const center = clampCamera(-1e9, 1e9, metrics, 1000, 800, world);
  assert.equal(center.x * 2 ** metrics.lod, world.width / 2);
  assert.equal(center.y * 2 ** metrics.lod, world.height / 2);
});
