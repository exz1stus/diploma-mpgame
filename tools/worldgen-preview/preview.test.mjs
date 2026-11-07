import test from 'node:test';
import assert from 'node:assert/strict';
import { SAMPLE_BYTES, decodePixels, readSample, readMapMode } from './dist/protocol.mjs';
import { zoomMetrics, fitWorldZoom, clampCamera } from './dist/zoom.mjs';
import { defaultSettings, settingsError, readSettings, settingsQuery } from './dist/settings.mjs';
import { biomeAt, biomeForSample, temperatureDomain } from './dist/biomes.mjs';

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

test('crater bounds are validated together and restored safely from invalid URLs', () => {
  const craterControls = [
    { key: 'crater_cell_size', label: 'Cell size', default: 512, min: 32, max: 4096, step: '1' },
    { key: 'crater_min_radius', label: 'Minimum radius', default: 32, min: 1, max: 2048, step: 'any' },
    { key: 'crater_max_radius', label: 'Maximum radius', default: 128, min: 1, max: 2048, step: 'any' },
  ];
  const defaults = defaultSettings(craterControls);
  assert.match(settingsError({ ...defaults, crater_min_radius: 256 }, craterControls), /Minimum/);
  assert.match(settingsError({ ...defaults, crater_cell_size: 32 }, craterControls), /cell size/);
  assert.deepEqual(readSettings(new URLSearchParams('crater_cell_size=32'), craterControls), defaults);
  const chosen = { crater_cell_size: 2048, crater_min_radius: 512, crater_max_radius: 1024 };
  assert.deepEqual(readSettings(settingsQuery(chosen, craterControls, true), craterControls), chosen);
});

test('basin settings round-trip, reject inconsistent ranges, and restore invalid URLs', () => {
  const basinControls = [
    { key: 'basin_cell_size', label: 'Cell size', default: 8192, min: 128, max: 65536, step: '1' },
    { key: 'basin_min_radius', label: 'Minimum radius', default: 800, min: 1, max: 16384, step: 'any' },
    { key: 'basin_max_radius', label: 'Maximum radius', default: 2400, min: 1, max: 16384, step: 'any' },
    { key: 'basin_min_depth', label: 'Minimum depth', default: 0.06, min: 0, max: 1, step: 'any' },
    { key: 'basin_max_depth', label: 'Maximum depth', default: 0.18, min: 0, max: 1, step: 'any' },
    { key: 'basin_min_rim_height', label: 'Minimum rim', default: 0.005, min: 0, max: 1, step: 'any' },
    { key: 'basin_max_rim_height', label: 'Maximum rim', default: 0.02, min: 0, max: 1, step: 'any' },
    { key: 'basin_probability', label: 'Probability', default: 0.12, min: 0, max: 1, step: 'any' },
    { key: 'basin_smoothing', label: 'Smoothing', default: 0.8, min: 0, max: 1, step: 'any' },
    { key: 'basin_coarse_octaves', label: 'Octaves', default: 2, min: 1, max: 12, step: '1' },
  ];
  const defaults = defaultSettings(basinControls);
  const chosen = { ...defaults, basin_probability: 1, basin_smoothing: 0.5, basin_coarse_octaves: 3 };
  assert.deepEqual(readSettings(settingsQuery(chosen, basinControls, true), basinControls), chosen);
  assert.match(settingsError({ ...defaults, basin_min_radius: 3000 }, basinControls), /Minimum basin radius/);
  assert.match(settingsError({ ...defaults, basin_cell_size: 128 }, basinControls), /cell size/);
  assert.match(settingsError({ ...defaults, basin_min_depth: 0.5 }, basinControls), /depth/);
  assert.match(settingsError({ ...defaults, basin_min_rim_height: 0.5 }, basinControls), /rim height/);
  assert.match(settingsError({ ...defaults, basin_probability: 2 }, basinControls), /Probability/);
  assert.deepEqual(readSettings(new URLSearchParams('basin_min_depth=0.5'), basinControls), defaults);
  const mixed = [...controls, ...basinControls];
  const restored = readSettings(new URLSearchParams('temperature_base=-42&basin_cell_size=128'), mixed);
  assert.equal(restored.temperature_base, -42);
  assert.equal(restored.basin_cell_size, defaults.basin_cell_size);
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

test('metric maps use sample values independently of biome colors and classification', () => {
  const buffer = new ArrayBuffer(4 * SAMPLE_BYTES);
  const bytes = new Uint8Array(buffer);
  const view = new DataView(buffer);
  for (let i = 0; i < 4; i++) {
    bytes.set([i === 0 ? 0 : i === 3 ? 255 : 1, 12, 34, 56], i * SAMPLE_BYTES);
    view.setFloat32(i * SAMPLE_BYTES + 4, i / 2, true);
    view.setFloat32(i * SAMPLE_BYTES + 8, -50 + i * 50, true);
  }
  const original = bytes.slice();
  const height = decodePixels(buffer, 2, 'height');
  const temperature = decodePixels(buffer, 2, 'temperature');
  assert.deepEqual([...height], [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255, 0, 0, 0, 0]);
  assert.deepEqual([...temperature], [40, 70, 160, 255, 240, 245, 235, 255, 190, 45, 45, 255, 0, 0, 0, 0]);
  assert.deepEqual(bytes, original, 'rendering must preserve raw hover metrics');
  for (let i = 0; i < 3; i++) bytes.set([1, 255, 0, 255], i * SAMPLE_BYTES);
  assert.deepEqual(decodePixels(buffer, 2, 'height'), height);
  assert.deepEqual(decodePixels(buffer, 2, 'temperature'), temperature);
  view.setFloat32(4, 0.75, true);
  assert.deepEqual(decodePixels(buffer, 2, 'temperature'), temperature, 'temperature colors must not depend on height');
  view.setFloat32(4, 0, true);
  view.setFloat32(8, 25, true);
  assert.deepEqual(decodePixels(buffer, 2, 'height'), height, 'height colors must not depend on temperature');
});

test('metric scales clamp extremes, preserve padding, and handle missing values', () => {
  const buffer = new ArrayBuffer(4 * SAMPLE_BYTES);
  const bytes = new Uint8Array(buffer);
  const view = new DataView(buffer);
  for (const [i, value] of [-1000, 1000, NaN, Infinity].entries()) {
    bytes[i * SAMPLE_BYTES] = i === 3 ? 255 : 0;
    view.setFloat32(i * SAMPLE_BYTES + 4, value, true);
    view.setFloat32(i * SAMPLE_BYTES + 8, value, true);
  }
  assert.deepEqual([...decodePixels(buffer, 2, 'height')], [0, 0, 0, 255, 255, 255, 255, 255, 54, 43, 61, 255, 0, 0, 0, 0]);
  assert.deepEqual([...decodePixels(buffer, 2, 'temperature')], [40, 70, 160, 255, 190, 45, 45, 255, 54, 43, 61, 255, 0, 0, 0, 0]);
  for (const mode of ['biomes', 'temperature', 'height']) {
    assert.equal(readMapMode(new URLSearchParams({ mode }).get('mode')), mode);
  }
  for (const mode of [null, '', 'unknown', 'constructor', '__proto__']) assert.equal(readMapMode(mode), 'biomes');
  assert.deepEqual(decodePixels(buffer, 2, 'unknown'), decodePixels(buffer, 2));
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

test('biome ranges use both axes, exclusive upper bounds, and definition order', () => {
  const low = { name: 'low', color: '#abc', temperature: { min: -20, max: 10 }, height: { min: 0, max: 0.5 } };
  const high = { name: 'high', color: '#123456', temperature: { min: -20, max: 10 }, height: { min: 0.5, max: 1 } };
  const overlap = { ...high, name: 'overlap', height: { min: 0, max: 1 } };
  const biomes = [low, high, overlap];
  assert.equal(biomeAt(biomes, 0, 0.49), low);
  assert.equal(biomeAt(biomes, -20, 0.5), high);
  assert.equal(biomeAt(biomes, 10, 0.5), undefined);
  assert.equal(biomeAt(biomes, 0, 1), undefined);
  assert.deepEqual(temperatureDomain(biomes), { min: -20, max: 10 });
  assert.ok(Number.isFinite(temperatureDomain([]).min));
  assert.equal(biomeForSample(biomes, { status: 1, color: '#aabbcc', temperature: 0, height: 0.499 }), low);
  assert.equal(biomeForSample(biomes, { status: 1, color: '#aabbcc', temperature: 0, height: 0.5 }), low);
  assert.equal(biomeForSample(biomes, { status: 0, color: '#aabbcc', temperature: 0, height: 0.49 }), null);
});

test('volcano controls round-trip and reject invalid radius, height and caldera settings', () => {
  const volcanoControls = [
    { key: 'volcano_cell_size', label: 'Cell size', default: 4096, min: 32, max: 65536, step: '1' },
    { key: 'volcano_min_radius', label: 'Minimum radius', default: 128, min: 1, max: 16384, step: 'any' },
    { key: 'volcano_max_radius', label: 'Maximum radius', default: 1024, min: 1, max: 16384, step: 'any' },
    { key: 'volcano_probability', label: 'Probability', default: 0.15, min: 0, max: 1, step: 'any' },
    { key: 'volcano_min_height', label: 'Minimum height', default: 0.15, min: 0, max: 1, step: 'any' },
    { key: 'volcano_max_height', label: 'Maximum height', default: 0.4, min: 0, max: 1, step: 'any' },
    { key: 'volcano_caldera_ratio', label: 'Caldera radius', default: 0.2, min: 0, max: 0.9, step: 'any' },
    { key: 'volcano_caldera_depth', label: 'Caldera depth', default: 0.15, min: 0, max: 1, step: 'any' },
  ];
  const defaults = defaultSettings(volcanoControls);
  const chosen = { ...defaults, volcano_probability: 1, volcano_cell_size: 1024, volcano_max_radius: 1024, volcano_caldera_ratio: 0.3 };
  assert.equal(settingsError(chosen, volcanoControls), '');
  assert.deepEqual(readSettings(settingsQuery(chosen, volcanoControls, true), volcanoControls), chosen);
  assert.match(settingsError({ ...defaults, volcano_cell_size: 128 }, volcanoControls), /cell size/);
  assert.match(settingsError({ ...defaults, volcano_min_radius: 2048 }, volcanoControls), /Minimum volcano radius/);
  assert.match(settingsError({ ...defaults, volcano_min_height: 0.6 }, volcanoControls), /Minimum volcano height/);
  assert.match(settingsError({ ...defaults, volcano_caldera_ratio: 1 }, volcanoControls), /Caldera radius/);
  const restored = readSettings(new URLSearchParams('temperature_base=-42&volcano_min_height=0.6'), [...controls, ...volcanoControls]);
  assert.equal(restored.temperature_base, -42);
  for (const key of Object.keys(defaults)) assert.equal(restored[key], defaults[key]);
});
