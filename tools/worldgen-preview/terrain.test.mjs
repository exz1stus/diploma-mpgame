import test from 'node:test';
import assert from 'node:assert/strict';
import { buildTileData, composeChunk, edgeNoise, lattice, sanitizeBlend } from './dist/terrain.mjs';
import {
  bakeObjectTexture, bandedPresentation, defaultLayers, defaultTileSource, drawnTexture, objectsRon, parseRon,
  stackLayers, tileCatalog, tilesRon,
} from './dist/presentation.mjs';

// No edge noise, no height shadow, no tint smoothing.
const NO_NOISE = { pixels_per_tile: 2, smooth_width: 1, overlay_reach: 0, edge_noise: 0, noise_scale: 0.5, ao_strength: 0, ao_width: 0, tint_radius: 0 };
const WHITE = [255, 255, 255];

// Solid 2 × 2 textures: index i is gray level 40 * (i + 1).
function textures(count) {
  return { width: 2, height: 2, layers: Array.from({ length: count }, (_, i) => Uint8Array.from({ length: 16 }, (_, c) => (c % 4 === 3 ? 255 : 40 * (i + 1)))) };
}

// A row of tiles described per column: [texture, overlay, ...tint (white when omitted)];
// every row is the same.
function chunk(columns, { originX = 0, blend = NO_NOISE, neighbors = null, layers = textures(4) } = {}) {
  const size = columns.length;
  const full = tile => tile && (tile.length === 5 ? tile : [...tile, ...WHITE]);
  const data = buildTileData(size, (x, y) => full(x >= 0 && x < size ? columns[x] : neighbors?.(x, y) ?? null), sanitizeBlend(blend).tint_radius);
  return composeChunk({ size, originX, originY: 0, data, textures: layers, blend });
}

const pixel = ({ pixels, width }, x, y) => [...pixels.slice((y * width + x) * 4, (y * width + x) * 4 + 4)];

test('the hash matches the shader\'s u32 arithmetic', () => {
  // Reference values computed with explicit 32-bit unsigned arithmetic (WGSL semantics).
  const expected = [[0, 0, 0, 2328593439], [1, 0, 0, 674391076], [-5, 7, 1, 2054654740], [123456, -98765, 0, 2086157518], [-2147483648, 2147483647, 1, 1619238147]];
  for (const [x, y, seed, value] of expected) assert.equal(lattice(x, y, seed) * 4294967296, value);
  for (let i = 0; i < 200; i++) {
    const noise = edgeNoise(i * 0.37 - 20, i * -0.53 + 7, 0.5);
    assert.ok(noise >= -0.5 && noise <= 0.5);
  }
});

test('blend settings clamp like the client', () => {
  assert.deepEqual(sanitizeBlend({ pixels_per_tile: 0, smooth_width: 3, overlay_reach: -1, edge_noise: NaN, noise_scale: 0, ao_width: 40, tint_radius: 99 }),
    { pixels_per_tile: 1, smooth_width: 1, overlay_reach: 0, edge_noise: 0.35, noise_scale: 0.05, ao_strength: 0.35, ao_width: 16, tint_radius: 12 });
});

test('textures repeat in world space with image rows top-down and world +Y up', () => {
  // One 2 × 2 texture with distinct texels: top row 10, 20; bottom row 30, 40.
  const layer = Uint8Array.from([10, 20, 30, 40].flatMap(value => [value, value, value, 255]));
  const image = chunk([[0, 0], [0, 0]], { blend: { ...NO_NOISE, pixels_per_tile: 2 }, layers: { width: 2, height: 2, layers: [layer] } });
  assert.equal(image.width, 4);
  // Output row 0 is the bottom of the chunk: texture row 1 (bottom), then row 0 above it.
  assert.deepEqual([0, 1, 2, 3].map(x => pixel(image, x, 0)[0]), [30, 40, 30, 40]);
  assert.deepEqual([0, 1, 2, 3].map(x => pixel(image, x, 1)[0]), [10, 20, 10, 20]);
});

test('a higher layer overlays a lower one with a crisp edge, reaching further with overlay reach', () => {
  const columns = [[0, 1], [0, 1], [1, 0], [1, 0]];
  const crisp = chunk(columns);
  const row = image => [...Array(image.width).keys()].map(x => pixel(image, x, 3)[0]);
  // Two pixels per tile: the edge falls exactly on the tile border, with no mixed colors.
  assert.deepEqual(row(crisp), [40, 40, 40, 40, 80, 80, 80, 80]);
  const reaching = chunk(columns, { blend: { ...NO_NOISE, pixels_per_tile: 8, overlay_reach: 0.25 } });
  const values = row(reaching);
  assert.ok(values.every(value => value === 40 || value === 80), 'overlay edges never mix');
  assert.ok(values.filter(value => value === 40).length > 16, 'the higher texture reaches past the tile border');
  const noisy = chunk(columns, { blend: { ...NO_NOISE, pixels_per_tile: 8, edge_noise: 1 } });
  const edges = [...Array(noisy.width).keys()].map(y => [...Array(noisy.width).keys()].findIndex(x => pixel(noisy, x, y)[0] === 80));
  assert.ok(new Set(edges).size > 1, 'edge noise makes the border irregular');
});

test('textures on the same layer fade smoothly', () => {
  const image = chunk([[0, 2], [0, 2], [1, 2], [1, 2]], { blend: { ...NO_NOISE, pixels_per_tile: 8 } });
  const row = [...Array(image.width).keys()].map(x => pixel(image, x, 3)[0]);
  assert.ok(row.some(value => value > 40 && value < 80), 'mixed colors across the border');
  for (let i = 1; i < row.length; i++) assert.ok(row[i] >= row[i - 1], 'monotonic fade');
  const crisp = chunk([[0, 2], [0, 2], [1, 2], [1, 2]], { blend: { ...NO_NOISE, pixels_per_tile: 8, smooth_width: 0 } });
  assert.ok([...Array(crisp.width).keys()].every(x => [40, 80].includes(pixel(crisp, x, 3)[0])), 'zero width is crisp');
});

test('lower ground at the foot of a higher layer is shaded in whole-pixel steps', () => {
  const columns = [[0, 1], [0, 1], [0, 0], [0, 0]];
  const blend = { ...NO_NOISE, pixels_per_tile: 8, ao_strength: 0.6, ao_width: 3 };
  const image = chunk(columns, { blend });
  const row = [...Array(image.width).keys()].map(x => pixel(image, x, 3)[0]);
  // Texture 0 everywhere (gray 40); the higher layer ends at x = 16.
  assert.deepEqual(row.slice(0, 16), Array(16).fill(40), 'the higher layer is not shaded');
  assert.ok(row[16] < row[17] && row[17] < row[18] && row[18] < row[19], `three darkening steps: ${row.slice(16, 20)}`);
  assert.deepEqual(row.slice(19), Array(13).fill(40), 'ground beyond the shadow is unchanged');
  const flat = chunk(columns, { blend: { ...blend, ao_strength: 0 } });
  assert.ok([...Array(flat.width).keys()].every(x => pixel(flat, x, 3)[0] === 40));
});

test('tints multiply the ground and are smoothed across tiles into a gradient', () => {
  // White texture; tint jumps from red to blue between columns 3 and 4.
  const layers = { width: 2, height: 2, layers: [new Uint8Array(16).fill(255)] };
  const columns = [...Array(8)].map((_, x) => [0, 0, ...(x < 4 ? [200, 0, 0] : [0, 0, 200])]);
  const sharp = chunk(columns, { layers, blend: { ...NO_NOISE, pixels_per_tile: 4 } });
  assert.deepEqual(pixel(sharp, 2, 5).slice(0, 3), [200, 0, 0], 'a white texture shows the tint');
  const smooth = chunk(columns, { layers, blend: { ...NO_NOISE, pixels_per_tile: 4, tint_radius: 3 } });
  const reds = [...Array(smooth.width).keys()].map(x => pixel(smooth, x, 5)[0]);
  for (let i = 1; i < reds.length; i++) assert.ok(reds[i] <= reds[i - 1], 'monotonic gradient');
  const mixed = reds.filter(red => red > 10 && red < 190).length;
  assert.ok(mixed >= 20, `the gradient spans several tiles (${mixed} px)`);
});

test('chunks are seamless: two halves with neighbor borders match one chunk', () => {
  const columns = [[0, 0], [2, 1], [1, 3], [3, 1]];
  const blend = { ...NO_NOISE, pixels_per_tile: 4, edge_noise: 0.6, overlay_reach: 0.1, ao_strength: 0.5, ao_width: 2, tint_radius: 1 };
  columns.forEach((column, x) => column.push(40 * x, 255 - 30 * x, 90));
  const whole = chunk(columns, { blend });
  const left = chunk(columns.slice(0, 2), { blend, neighbors: x => columns[x] ?? null });
  const right = chunk(columns.slice(2), { blend, originX: 2, neighbors: x => columns[x + 2] ?? null });
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
    assert.deepEqual(pixel(left, x, y), pixel(whole, x, y), `left ${x}, ${y}`);
    assert.deepEqual(pixel(right, x, y), pixel(whole, x + 8, y), `right ${x}, ${y}`);
  }
});

test('missing neighbors repeat the edge tile and hidden tiles stay transparent', () => {
  const inside = (x, y) => x >= 0 && x < 4 && y >= 0 && y < 4;
  const data = buildTileData(4, (x, y) => (inside(x, y) ? [x + 4 * y, 7, ...WHITE] : null));
  assert.equal(data.layer.length, 6 * 6, 'one-tile border around 4 × 4 tiles');
  assert.equal(data.layer[0], 0, 'corner border repeats tile (0, 0)');
  assert.equal(data.layer[35], 15, 'opposite corner repeats tile (3, 3)');
  assert.equal(data.layer[1 * 6 + 2], 1, 'tile (1, 0) sits one texel in');
  assert.equal(data.level[14], 7);
  const image = composeChunk({ size: 4, originX: 0, originY: 0, data, textures: textures(16), blend: NO_NOISE, hidden: x => x === 1 });
  assert.equal(pixel(image, 3, 0)[3], 0);
  assert.equal(pixel(image, 0, 0)[3], 255);
});

test('catalogs resolve biomes like the client: shared textures, ground fallback, layers', () => {
  const tiles = { textures: { volcano: 'basalt', ash: 'basalt' }, default_placeholder: '#FF00FF', placeholder_colors: { basalt: '#202020' }, layers: { basalt: 2 } };
  const catalog = tileCatalog(['plain', 'volcano', 'ash', 'crater'], tiles, name => name === 'plain');
  assert.deepEqual(catalog.textures.map(texture => texture.name), ['plain', 'basalt', 'crater']);
  assert.deepEqual(catalog.ownTiles, [true, true, true, false]);
  assert.deepEqual(catalog.overlay, [0, 2, 0]);
  assert.equal(catalog.textures[0].placeholder, '#FF00FF');
  assert.equal(drawnTexture(catalog, 3, 1), 1, 'unlisted crater draws its ground');
  assert.equal(drawnTexture(catalog, 2, 0), 1);
});

test('default layers stack by height within a temperature column', () => {
  const rule = (name, h0, h1, t0, t1) => ({ name, height: { min: h0, max: h1 }, temperature: { min: t0, max: t1 } });
  const terrain = [rule('low', 0, 0.5, 0, 10), rule('high', 0.5, 1, 0, 10), rule('warm_low', 0, 0.4, 10, 20), rule('warm_high', 0.4, 1, 10, 20)];
  const layers = defaultLayers(terrain, [[rule('volcano', 0, 1, -50, 50)], [rule('crater', 0, 0.5, -50, 50), rule('rim', 0.5, 1, -50, 50)]], { textures: {} });
  assert.deepEqual(layers, { low: 1, high: 2, warm_low: 1, warm_high: 2, volcano: 3, crater: 3, rim: 4 });
});

test('legacy colored biomes become neutral height-band textures with tints', () => {
  const rule = (name, h0, h1, t0, t1) => ({ name, height: { min: h0, max: h1 }, temperature: { min: t0, max: t1 } });
  const terrain = [rule('ice_low', 0, 0.5, -40, -20), rule('ice_high', 0.5, 1, -40, -20), rule('dust', 0, 0.5, 0, 20), rule('ridge', 0.5, 1, 0, 20)];
  const legacy = { textures: {}, default_placeholder: '#FF00FF', placeholder_colors: { ice_low: '#AACCEE', ice_high: '#EEEEFF', dust: '#A18B68', ridge: '#ABA18B', volcano: '#454850' } };
  const tiles = bandedPresentation(legacy, terrain, [[rule('volcano', 0, 1, -50, 50)]], ['ice_low', 'ice_high', 'dust', 'ridge', 'volcano', 'crater']);
  assert.deepEqual(tiles.textures, { ice_low: 'frost_1', ice_high: 'frost_2', dust: 'regolith_1', ridge: 'regolith_2', volcano: 'basalt_3' });
  assert.deepEqual(tiles.layers, { frost_1: 1, frost_2: 2, regolith_1: 1, regolith_2: 2, basalt_3: 3 });
  assert.equal(tiles.tints.dust, '#A18B68');
  assert.equal(tiles.textures.crater, undefined, 'unlisted feature biomes keep drawing their ground');
  assert.deepEqual(defaultTileSource('regolith_1'), { id: 'regolith_lowland', tint: 'tint', strength: 1 });
  assert.equal(defaultTileSource('frost_3').id, 'snow');
  assert.equal(defaultTileSource('custom'), null);
  const catalog = tileCatalog(['dust', 'crater'], tiles, () => false);
  assert.deepEqual(catalog.tints, [[0xA1, 0x8B, 0x68], [255, 255, 255]]);
});

test('exported RON parses back and keeps every setting', () => {
  const tiles = { textures: { ash: 'basalt' }, default_placeholder: '#FF00FF', placeholder_colors: { basalt: '#202020', 'odd "name"': '#000000' }, layers: { basalt: 3 }, tints: { ash: '#AA8866' }, blend: { pixels_per_tile: 16, smooth_width: 0.5, overlay_reach: 0.2, edge_noise: 0.35, noise_scale: 0.5, ao_strength: 0.4, ao_width: 2, tint_radius: 6 } };
  assert.deepEqual(parseRon(tilesRon(tiles)), tiles);
  const objects = { textures: {}, default_placeholder: '#475A47', placeholder_colors: { rock: '#5A5550' } };
  assert.deepEqual(parseRon(objectsRon(objects)), objects);
  assert.deepEqual(parseRon('( a: Some(1.5), b: None, /* c */ c: [true, -2e3], d: {"k": (x: 1)}, ) // end'),
    { a: 1.5, b: null, c: [true, -2000], d: { k: { x: 1 } } });
  assert.throws(() => parseRon('(a: 1'), /RON/);
});

test('texture arrays use placeholders for missing and mismatched images', () => {
  const entries = [{ name: 'a', placeholder: '#FF0000' }, { name: 'b', placeholder: '#0000FF' }, { name: 'c', placeholder: '#00FF00' }];
  const image = (size, value) => ({ width: size, height: size, data: new Uint8ClampedArray(size * size * 4).fill(value) });
  const stack = stackLayers(entries, [null, image(2, 9), image(4, 1)]);
  assert.equal(stack.width, 2);
  assert.deepEqual([...stack.layers[0].slice(0, 4)], [255, 0, 0, 255]);
  assert.deepEqual([...stack.layers[1].slice(0, 4)], [9, 9, 9, 9]);
  assert.deepEqual([...stack.layers[2].slice(0, 4)], [0, 255, 0, 255]);
  assert.equal(stack.warnings.length, 1);
  const empty = stackLayers([entries[0]], [null]);
  assert.equal(empty.width, 1);
});

test('object textures are baked at footprint size, standing on the bottom edge', () => {
  const sheet = { width: 512, height: 512, data: new Uint8ClampedArray(512 * 512 * 4) };
  // Opaque rock_0 sprite region (15 × 16 at 8, 424).
  for (let y = 424; y < 440; y++) for (let x = 8; x < 23; x++) sheet.data.set([100, 100, 100, 255], (y * 512 + x) * 4);
  const baked = bakeObjectTexture(sheet, { id: 'rock_0', tint: 'none' }, '#FFFFFF', { width: 2, height: 2 }, 16);
  assert.equal(baked.width, 32);
  assert.equal(baked.height, 32);
  const alpha = (x, y) => baked.data[(y * 32 + x) * 4 + 3];
  assert.equal(alpha(16, 31), 255, 'bottom row is covered');
  assert.equal(alpha(16, 0), 0, 'space above the sprite stays clear');
  const shrunk = bakeObjectTexture(sheet, { id: 'rock_0', tint: 'none' }, '#FFFFFF', { width: 1, height: 1 }, 8);
  assert.equal(shrunk.width, 8);
});
