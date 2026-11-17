import test from 'node:test';
import assert from 'node:assert/strict';
import { SAMPLE_BYTES } from './dist/protocol.mjs';
import { SURFACE, SPRITES, SURFACE_TEXTURES, isBasaltTile, objectIconSprite, objectSprite, pebbleChance, surfaceFor } from './dist/sprites.mjs';
import { buildGrid, composeTerrain, terrainTilePx, textureStats } from './dist/terrain.mjs';

// Synthetic sheet: every texture is a gray or colored checker with a known mean.
const atlas = { width: 448, height: 512, data: new Uint8ClampedArray(448 * 512 * 4) };
for (let y = 0; y < atlas.height; y++) for (let x = 0; x < atlas.width; x++) {
  const offset = (y * atlas.width + x) * 4;
  const value = (x + y) % 2 ? 90 : 70;
  const ice = y >= 128 && y < 160 && x >= 416;
  atlas.data.set([value, value + (ice ? 30 : 0), value + (ice ? 90 : y >= 256 && y < 288 ? 10 : 0), 255], offset);
}
const stats = textureStats(atlas);
const tiles = [{ tint: [200, 100, 50], basalt: false }, { tint: [80, 90, 100], basalt: true }];

function world(fn) {
  return (cx, cy, size) => {
    const bytes = new Uint8Array(size * size * SAMPLE_BYTES);
    const view = new DataView(bytes.buffer);
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
      const { status = 1, tile = 0, height = .45, temperature = 10 } = fn(cx * size + x, cy * size + y);
      const offset = (y * size + x) * SAMPLE_BYTES;
      bytes[offset] = status;
      view.setFloat32(offset + 4, height, true);
      view.setFloat32(offset + 8, temperature, true);
      view.setUint16(offset + 14, tile, true);
    }
    return bytes;
  };
}

function compose(sample, cx, cy, size, sheet = atlas, display = { height_tint: 0, relief: 0 }) {
  const neighbors = [];
  for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) neighbors.push(sample(cx + dx, cy + dy, size));
  return composeTerrain({ grid: buildGrid(neighbors, size, tiles), size, tilePx: 16, chunkX: cx, chunkY: cy, atlas: sheet, stats, display });
}

test('surfaces follow cold, basalt, and height rules', () => {
  assert.equal(surfaceFor(false, .7, -20), SURFACE.SNOW);
  assert.equal(surfaceFor(true, .3, -20), SURFACE.ICE, 'very cold ground freezes even on basalt');
  assert.equal(surfaceFor(false, .5, -20), SURFACE.ICE);
  assert.equal(surfaceFor(false, .5, -12), SURFACE.GRAY_3, 'the cold limit is exclusive');
  assert.equal(surfaceFor(true, .9, 4), SURFACE.BASALT);
  // The gray strip runs from high (left) to low (right) terrain.
  assert.deepEqual([.9, .7, .6, .3].map(h => surfaceFor(false, h, 5)),
    [SURFACE.GRAY_0, SURFACE.GRAY_1, SURFACE.GRAY_2, SURFACE.GRAY_3]);
  assert.deepEqual([0, 1, 2, 3].map(i => SURFACE_TEXTURES[i].origin[0]), [288, 320, 352, 384]);
  assert.equal(SURFACE_TEXTURES[SURFACE.ICE].origin.join(), '416,128');
  assert.ok(pebbleChance(.4) > pebbleChance(.7) && pebbleChance(.9) === 0, 'lower ground is rockier');
  assert.ok(isBasaltTile('weathered_basalt') && isBasaltTile('volcano_2') && isBasaltTile('volcanic_highlands_0'));
  assert.ok(!isBasaltTile('ochre_dust_plains'));
  assert.equal(terrainTilePx(32), 16);
  assert.equal(terrainTilePx(128), 8);
});

test('uniform gray ground averages to the biome tint', () => {
  const mean = meanColor(compose(world(() => ({ height: .9 })), 3, 3, 4));
  const brightness = stats[SURFACE.GRAY_0].brightness;
  tiles[0].tint.forEach((value, c) => assert.ok(Math.abs(mean[c] - value * brightness) < 2, `${mean} vs ${tiles[0].tint}`));
});

function meanColor(pixels) {
  const mean = [0, 0, 0];
  for (let i = 0; i < pixels.length; i += 4) for (let c = 0; c < 3; c++) mean[c] += pixels[i + c] / (pixels.length / 4);
  return mean;
}

test('pebble overlays appear on low ground but not on high ground, snow, or ice', () => {
  // Same sheet with transparent pebble cells: any difference is a pebble overlay.
  const bare = { ...atlas, data: atlas.data.slice() };
  for (let y = 384; y < 416; y++) for (let x = 0; x < 256; x++) bare.data[(y * atlas.width + x) * 4 + 3] = 0;
  const differs = fn => {
    const sample = world(fn);
    const a = compose(sample, 3, 3, 8);
    const b = compose(sample, 3, 3, 8, bare);
    return a.some((value, i) => value !== b[i]);
  };
  assert.ok(differs(() => ({ height: .3 })), 'low ground gets pebbles');
  assert.ok(!differs(() => ({ height: .9 })), 'high ground stays clear');
  assert.ok(!differs(() => ({ height: .3, temperature: -30 })), 'no pebbles on ice');
  assert.ok(!differs(() => ({ height: .8, temperature: -30 })), 'no pebbles on snow');
});

test('height tint makes low ground darker in textured chunks', () => {
  const tinted = { height_tint: 3, relief: 0 };
  const high = meanColor(compose(world(() => ({ height: .9, temperature: -30 })), 3, 3, 4, atlas, tinted));
  const low = meanColor(compose(world(() => ({ height: .4, temperature: -30 })), 3, 3, 4, atlas, tinted));
  assert.ok(high[0] > low[0] * 1.5, `${high} vs ${low}`);
});

test('ice keeps its blue texture under a warm tint', () => {
  const [red, , blue] = meanColor(compose(world(() => ({ height: .3, temperature: -30 })), 3, 3, 4));
  assert.ok(blue > red, `ice should stay blue, got ${red} / ${blue}`);
});

test('chunk images are seamless: split chunks match one larger chunk', () => {
  const sample = world((x, y) => ({
    tile: (x * 7 + y * 3) % 5 === 0 ? 1 : 0,
    height: ((x * 13 + y * 5) % 10) / 10,
    temperature: x + y > 12 ? -20 : 5,
  }));
  const large = compose(sample, 1, 1, 8);
  for (const [qx, qy] of [[2, 2], [3, 2], [2, 3], [3, 3]]) {
    const small = compose(sample, qx, qy, 4);
    for (let y = 0; y < 64; y++) for (let x = 0; x < 64; x++) {
      const a = (y * 64 + x) * 4;
      const b = (((qy - 2) * 64 + y) * 128 + (qx - 2) * 64 + x) * 4;
      assert.deepEqual([...small.subarray(a, a + 4)], [...large.subarray(b, b + 4)], `pixel ${x},${y} of chunk ${qx},${qy}`);
    }
  }
});

test('outside-world padding stays transparent', () => {
  const pixels = compose(world(x => (x % 4 < 2 ? { status: 255 } : {})), 0, 0, 4);
  assert.equal(pixels[3], 0);
  assert.equal(pixels[(3 * 16) * 4 + 3], 255);
});

test('object sprites are stable per instance and switch to stalagmites on basalt', () => {
  const object = { id: { rule_id: 2, anchor: { x: 40, y: 9 } } };
  const first = objectSprite('large_rock', object, SURFACE.GRAY_1);
  assert.deepEqual(objectSprite('large_rock', structuredClone(object), SURFACE.GRAY_1), first);
  assert.equal(first.set, 'boulders');
  assert.equal(objectSprite('large_rock', object, SURFACE.BASALT).set, 'stalagmitesLarge');
  assert.equal(objectSprite('rock', object, SURFACE.SNOW).set, 'rocks');
  assert.equal(objectSprite('iron_ore', object, SURFACE.BASALT).ore, true);
  assert.equal(objectSprite('tree', object, SURFACE.GRAY_1), null, 'trees keep their vector symbol');
  assert.equal(objectIconSprite('rock').rect, SPRITES.rocks[0]);
  for (const rects of Object.values(SPRITES)) for (const rect of rects) assert.ok(rect.width > 0 && rect.height > 0 && rect.width <= 32);
});
