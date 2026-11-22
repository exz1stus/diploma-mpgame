import test from 'node:test';
import assert from 'node:assert/strict';
import { buildTileData, composeChunk, edgeNoise, lattice, sanitizeBlend } from './dist/terrain.mjs';
import {
  bandedPresentation, cropRegion, defaultLayers, rampTint, hexRgb, migrateObjects, planetPresentation, PLANET_PALETTE, drawnTexture, objectsRon, parseRon,
  readAtlas, recolorImage, stackLayers, tileCatalog, tileRegion, tilesRon,
} from './dist/presentation.mjs';

// No edge noise, no height shadow, no tint smoothing.
const NO_NOISE = { pixels_per_tile: 2, overlay_reach: 0, edge_noise: 0, noise_scale: 0.5, ao_strength: 0, ao_width: 0, ao_south: 1, ao_north: 1, tint_radius: 0 };
const WHITE = [255, 255, 255];

// Solid 2 × 2 textures: index i is gray level 40 * (i + 1).
function textures(count) {
  return { width: 2, height: 2, layers: Array.from({ length: count }, (_, i) => Uint8Array.from({ length: 16 }, (_, c) => (c % 4 === 3 ? 255 : 40 * (i + 1)))) };
}

// A row of tiles described per column: [texture, overlay, ...tint (white when omitted),
// biome (0 when omitted)]; every row is the same.
function chunk(columns, { originX = 0, blend = NO_NOISE, neighbors = null, layers = textures(4) } = {}) {
  const size = columns.length;
  const full = tile => tile && (tile.length === 6 ? tile : tile.length === 5 ? [...tile, 0] : [...tile, ...WHITE, 0]);
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
  assert.deepEqual(sanitizeBlend({ pixels_per_tile: 0, overlay_reach: -1, edge_noise: NaN, noise_scale: 0, ao_width: 40, tint_radius: 99 }),
    { pixels_per_tile: 1, overlay_reach: 0, edge_noise: 0.35, noise_scale: 0.05, ao_strength: 0.35, ao_width: 16, ao_south: 1.5, ao_north: 0.4, tint_radius: 12 });
  assert.equal(sanitizeBlend({ ao_south: 9 }).ao_south, 4);
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

test('biomes on one layer meet on a crisp edge that edge noise makes irregular', () => {
  // Biome 1 (texture 0, gray 40) | biome 2 (texture 1, gray 80), same layer.
  const columns = [[0, 2, ...WHITE, 1], [0, 2, ...WHITE, 1], [1, 2, ...WHITE, 2], [1, 2, ...WHITE, 2]];
  const crisp = chunk(columns, { blend: { ...NO_NOISE, pixels_per_tile: 8 } });
  const row = [...Array(crisp.width).keys()].map(x => pixel(crisp, x, 3)[0]);
  assert.deepEqual(row, [...Array(16).fill(40), ...Array(16).fill(80)], 'no noise: a clean edge on the tile border, no mixed colors');
  const noisy = chunk(columns, { blend: { ...NO_NOISE, pixels_per_tile: 8, edge_noise: 0.6 } });
  const values = new Set();
  const edges = [];
  for (let y = 0; y < noisy.width; y++) {
    const line = [...Array(noisy.width).keys()].map(x => pixel(noisy, x, y)[0]);
    line.forEach(value => values.add(value));
    edges.push(line.indexOf(80));
  }
  assert.deepEqual([...values].sort(), [40, 80], 'still crisp: only the two textures');
  assert.ok(new Set(edges).size > 2, 'the edge wanders');
  // One texture, two biome tints: the tint steps crisply on the edge.
  const tinted = chunk([[0, 2, 200, 0, 0, 1], [0, 2, 200, 0, 0, 1], [0, 2, 0, 0, 200, 2], [0, 2, 0, 0, 200, 2]],
    { layers: { width: 2, height: 2, layers: [new Uint8Array(16).fill(255)] }, blend: { ...NO_NOISE, pixels_per_tile: 8 } });
  assert.deepEqual(pixel(tinted, 15, 3).slice(0, 3), [200, 0, 0]);
  assert.deepEqual(pixel(tinted, 16, 3).slice(0, 3), [0, 0, 200], 'tints switch on the biome edge');
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

// A column of pixels through a raised band of tiles (rows 3..4 on layer 1, the rest on 0),
// from the bottom (south) up.
function bandColumn(blend) {
  const size = 8;
  const data = buildTileData(size, (x, y) => [0, y >= 3 && y <= 4 ? 1 : 0, ...WHITE]);
  const image = composeChunk({ size, originX: 0, originY: 0, data, textures: textures(1), blend });
  return [...Array(image.width).keys()].map(y => pixel(image, 20, y)[0]);
}

test('the height shadow is stronger south of a raised band than north of it', () => {
  const column = bandColumn({ ...NO_NOISE, pixels_per_tile: 8, ao_strength: 0.4, ao_width: 3, ao_south: 1.5, ao_north: 0.4 });
  // Rows 0..23 lie south of the band (it is north of them); rows 40..63 lie north.
  const south = column.slice(0, 24);
  const north = column.slice(40);
  const shaded = values => values.filter(value => value < 40).length;
  assert.ok(shaded(south) > shaded(north), `wider on the south side: ${shaded(south)} vs ${shaded(north)} px`);
  assert.ok(Math.min(...south) < Math.min(...north), 'darker on the south side');
  assert.ok(shaded(north) > 0, 'the north side still has a little shade');
  const even = bandColumn({ ...NO_NOISE, pixels_per_tile: 8, ao_strength: 0.4, ao_width: 3, ao_south: 1, ao_north: 1 });
  assert.equal(shaded(even.slice(0, 24)), shaded(even.slice(40)), 'equal scales shade both sides alike');
  const none = bandColumn({ ...NO_NOISE, pixels_per_tile: 8, ao_strength: 0.4, ao_width: 3, ao_south: 1, ao_north: 0 });
  assert.equal(shaded(none.slice(40)), 0, 'north scale 0 removes the north shadow');
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

test('tints never cross a height step', () => {
  // White texture; the left half (layer 1, green tint) is higher than the right (layer 0, red).
  const layers = { width: 2, height: 2, layers: [new Uint8Array(16).fill(255)] };
  const columns = [...Array(8)].map((_, x) => (x < 4 ? [0, 1, 0, 200, 0] : [0, 0, 200, 0, 0]));
  const image = chunk(columns, { layers, blend: { ...NO_NOISE, pixels_per_tile: 4, tint_radius: 3 } });
  for (let x = 0; x < image.width; x++) {
    const [r, g] = pixel(image, x, 12);
    assert.ok(r === 0 || g === 0, `pixel ${x} mixes both tints: ${r}, ${g}`);
  }
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

test('legacy colored biomes become neutral height-band textures with ramp tints', () => {
  const rule = (name, h0, h1, t0, t1) => ({ name, height: { min: h0, max: h1 }, temperature: { min: t0, max: t1 } });
  const terrain = [rule('ice_low', 0, 0.5, -40, -20), rule('ice_high', 0.5, 1, -40, -20), rule('dust', 0, 0.5, 0, 20), rule('ridge', 0.5, 1, 0, 20)];
  const legacy = { textures: {}, default_placeholder: '#FF00FF', placeholder_colors: { ice_low: '#AACCEE', ice_high: '#EEEEFF', dust: '#A18B68', ridge: '#ABA18B', volcano: '#454850' } };
  const tiles = bandedPresentation(legacy, terrain, [[rule('volcano', 0, 1, -50, 50)]], ['ice_low', 'ice_high', 'dust', 'ridge', 'volcano', 'crater']);
  assert.deepEqual(tiles.textures, { ice_low: 'frost_1', ice_high: 'frost_2', dust: 'regolith_1', ridge: 'regolith_2', volcano: 'basalt_3' });
  assert.deepEqual(tiles.layers, { frost_1: 1, frost_2: 2, regolith_1: 1, regolith_2: 2, basalt_3: 3 });
  assert.equal(tiles.tints.dust, rampTint(10, 0.25), 'tints come from the temperature ramp at the biome midpoint');
  assert.equal(bandedPresentation({ ...legacy, tints: { dust: '#123456' } }, terrain, [], ['dust']).tints.dust, '#123456', 'existing tints are kept');
  assert.equal(tiles.textures.crater, undefined, 'unlisted feature biomes keep drawing their ground');
  const catalog = tileCatalog(['dust', 'crater'], tiles, () => false);
  assert.deepEqual(catalog.tints, [hexRgb(rampTint(10, 0.25)), [255, 255, 255]]);
});

test('the temperature ramp is gradual: cold is blue-white, warm is tan, neighbors are close', () => {
  const [r0, , b0] = hexRgb(rampTint(-40));
  assert.ok(b0 > r0, 'cold tints lean blue');
  const [r1, , b1] = hexRgb(rampTint(25));
  assert.ok(r1 > b1, 'warm tints lean red');
  let previous = hexRgb(rampTint(-50));
  for (let t = -48; t <= 45; t += 2) {
    const next = hexRgb(rampTint(t));
    assert.ok(next.every((value, c) => Math.abs(value - previous[c]) <= 6), `2 °C steps stay close at ${t} °C`);
    previous = next;
  }
  assert.ok(hexRgb(rampTint(0, 1))[0] > hexRgb(rampTint(0, 0))[0], 'higher ground is slightly lighter');
});

test('the planet preset gives every kind five darker-to-lighter height layers', () => {
  const tiles = planetPresentation(['snow_floor', 'snow_ridge', 'plain_mid', 'basalt_low', 'crater']);
  assert.deepEqual(tiles.layers, { snow_floor: 1, snow_ridge: 5, plain_mid: 3, basalt_low: 2 });
  assert.equal(tiles.tints.plain_mid, PLANET_PALETTE.plain[2]);
  assert.equal(tiles.tints.crater, undefined, 'crater territory draws its ground');
  for (const colors of Object.values(PLANET_PALETTE)) {
    const light = colors.map(hex => hexRgb(hex).reduce((sum, value) => sum + value, 0));
    for (let i = 1; i < light.length; i++) assert.ok(light[i] > light[i - 1], 'higher bands are lighter');
  }
});

test('exported RON parses back and keeps every setting', () => {
  const tiles = { textures: { ash: 'basalt' }, default_placeholder: '#FF00FF', placeholder_colors: { basalt: '#202020', 'odd "name"': '#000000' }, regions: { basalt: 'basalt_0' }, recolor: true, layers: { basalt: 3 }, tints: { ash: '#AA8866' }, blend: { pixels_per_tile: 16, overlay_reach: 0.2, edge_noise: 0.35, noise_scale: 0.5, ao_strength: 0.4, ao_width: 2, ao_south: 2, ao_north: 0.25, tint_radius: 6 } };
  assert.deepEqual(parseRon(tilesRon(tiles)), tiles);
  const objects = {
    default_placeholder: '#475A47',
    tints: { plain_mid: '#64625E' },
    shadow: { opacity: 0.3, max_length: 2.5, fade_degrees: 10, depth: 0.5 },
    occlusion: { opacity: 0.25, reach: 4, decal_reach: 2, base_rows: 3, base_opacity: 0.2 },
    recolor: '#EEEEEE',
    surfaces: { plain_mid: 'regolith', basalt_low: 'basalt' },
    kinds: {
      pebbles: { sprite: 'regolith_pebbles', sprites: { basalt: 'basalt_pebbles' }, jitter: 0.4, flip: true, layer: 'Ground', placeholder: '#77726B' },
      rock: { scale: [1, 1.25], tint: '#FFEEDD', tinted: false, shadow: false, occlusion: false },
      tree: {},
    },
  };
  assert.deepEqual(parseRon(objectsRon(objects)), objects);
  assert.deepEqual(parseRon('( a: Some(1.5), b: None, /* c */ c: [true, -2e3], d: {"k": (x: 1)}, ) // end'),
    { a: 1.5, b: null, c: [true, -2000], d: { k: { x: 1 } } });
  assert.deepEqual(parseRon('(rect: (1, 2, 3, 4), pivot: Some((1.5, 4)), layer: Ground, empty: ())'),
    { rect: [1, 2, 3, 4], pivot: [1.5, 4], layer: 'Ground', empty: {} }, 'tuples become lists, unit variants their name');
  assert.throws(() => parseRon('(a: 1'), /RON/);
});

test('tile art comes from atlas regions; old object files keep their colors', () => {
  const atlas = readAtlas('(image: "textures/atlas.png", tiles: {"basalt_0": (2, 0, 2, 1)}, sprites: {"rock": [(rect: (0, 0, 1, 1))]})');
  const tiles = { textures: {}, placeholder_colors: {}, default_placeholder: '#FF00FF', regions: { basalt_low: 'basalt_0', none: 'missing' } };
  assert.deepEqual(tileRegion(atlas, tiles, 'basalt_low'), [2, 0, 2, 1]);
  assert.deepEqual(tileRegion(atlas, tiles, 'basalt_0'), [2, 0, 2, 1], 'unlisted textures use their own name');
  assert.equal(tileRegion(atlas, tiles, 'none'), null);
  const catalog = tileCatalog(['basalt_low', 'crater'], tiles, name => !!tileRegion(atlas, tiles, name));
  assert.deepEqual(catalog.ownTiles, [true, false], 'an atlas region gives a biome its own tiles');
  // 4 × 1 sheet; each pixel's red channel is its index.
  const sheet = { width: 4, height: 1, data: Uint8ClampedArray.from({ length: 16 }, (_, i) => (i % 4 === 0 ? i / 4 : 255)) };
  assert.deepEqual([...cropRegion(sheet, [2, 0, 2, 1]).data].filter((_, i) => i % 4 === 0), [2, 3]);
  assert.equal(cropRegion(sheet, [3, 0, 2, 1]), null, 'regions outside the sheet are rejected');
  // Same case as the client's recolor test: mean red 125, so 100 → 128 and 200 clamps to white.
  const art = { width: 2, height: 2, data: Uint8ClampedArray.from([200, 50, 10, 255, 100, 50, 10, 255, 100, 50, 10, 255, 100, 50, 10, 255]) };
  const neutral = recolorImage(art, '#A0A0A0');
  assert.deepEqual([...neutral.data.slice(0, 8)], [255, 255, 255, 255, 128, 128, 128, 255]);
  const migrated = migrateObjects({ textures: { large_rock: 'rock' }, default_placeholder: '#475A47', placeholder_colors: { rock: '#5A5550' } });
  assert.deepEqual(migrated.kinds, { rock: { placeholder: '#5A5550' }, large_rock: { placeholder: '#5A5550', sprite: 'rock' } });
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

