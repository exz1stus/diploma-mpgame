import test from 'node:test';
import assert from 'node:assert/strict';
import { fnv1a, hash32, pickOre, placeObject, styleJitter, styleScale } from './dist/placement.mjs';
import { objectTint, objectVisual, oreTint, sortVisuals } from './dist/objects.mjs';
import { recolorRegions, spriteRects } from './dist/presentation.mjs';

test('tinted kinds take the biome object tint and the recolored sheet; untinted kinds keep their colors', () => {
  assert.equal(objectTint({}, null), null, 'white draws untinted');
  assert.deepEqual(objectTint({}, [200, 100, 50]), [200, 100, 50]);
  assert.deepEqual(objectTint({ tint: '#808080' }, [200, 100, 50]), [100, 50, 25]);
  const look = {
    sheet: {}, recolored: {}, tilesPerPx: 1 / 16,
    atlas: { sprites: { rock: [{ rect: [0, 0, 16, 16] }] } },
    kinds: [{ name: 'rock', style: {}, salt: 1, placeholder: '#5A5550' }, { name: 'crystal', style: { sprite: 'rock', tinted: false }, salt: 2, placeholder: '#9FB8D8' }],
  };
  const object = kind => ({ kind, id: { anchor: { x: 0, y: 0 } }, footprint: { width: 1, height: 1 } });
  const rock = objectVisual(object(0), look, { surface: null, tint: [10, 20, 30] });
  assert.deepEqual([rock.tinted, rock.tint], [true, [10, 20, 30]]);
  const crystal = objectVisual(object(1), look, { surface: null, tint: [10, 20, 30] });
  assert.deepEqual([crystal.tinted, crystal.tint], [false, null], 'untinted kinds ignore the biome tint');
  // Ores mix the biome tint toward the picked ore's color; untinted kinds take neither.
  const ores = { mix: 0.5, kinds: [['coal', '#000000'], ['iron', '#FF0000']] };
  const ore = { name: 'ore', style: { sprite: 'rock', ores }, salt: fnv1a('iron_ore') };
  const picked = pickOre({ x: 10, y: 20 }, ore.salt, 2);
  assert.deepEqual(oreTint(ore, { x: 10, y: 20 }, [100, 100, 100]), picked === 0 ? [50, 50, 50] : [178, 50, 50]);
  assert.deepEqual(oreTint(ore, { x: 10, y: 20 }, null), picked === 0 ? [128, 128, 128] : [255, 128, 128]);
  assert.deepEqual(oreTint({ ...ore, style: {} }, { x: 0, y: 0 }, [1, 2, 3]), [1, 2, 3]);
  const untintedOre = { ...ore, style: { ...ore.style, tinted: false } };
  assert.equal(objectVisual(object(0), { ...look, kinds: [untintedOre] }, { surface: null, tint: [10, 20, 30] }).tint, null);
  // Two 1-pixel sprites side by side are recolored on their own means (both become the color).
  const sheet = { width: 2, height: 1, data: Uint8ClampedArray.from([40, 0, 0, 255, 200, 9, 9, 255]) };
  const atlas = { sprites: { a: [{ rect: [0, 0, 1, 1] }, { rect: [1, 0, 1, 1] }], b: [{ rect: [1, 0, 1, 1] }] } };
  assert.equal(spriteRects(atlas).length, 2, 'shared rects are listed once');
  assert.deepEqual([...recolorRegions(sheet, spriteRects(atlas), '#646464').data], [100, 100, 100, 255, 100, 100, 100, 255]);
});

// The same vectors as crates/client/src/world/presentation/placement.rs; keep them in sync.
test('hashes match the client', () => {
  assert.equal(fnv1a(''), 0x811c9dc5);
  assert.equal(fnv1a('rock'), 0x3a1b4ab4);
  assert.equal(hash32(0, 0, 0), 0);
  assert.equal(hash32(1, 2, 3), 0x9d92aeb2);
  assert.equal(hash32(123456, 654321, fnv1a('rock')), 0x3fca3dae);
  assert.equal(pickOre({ x: 10, y: 20 }, fnv1a('iron_ore'), 7), 3);
  assert.equal(pickOre({ x: 10, y: 20 }, fnv1a('iron_ore'), 0), 0);
});

const variants = [[0, 0, 16, 16], [16, 0, 10, 20], [32, 0, 24, 40]].map(rect => ({ rect }));

test('placement matches the client', () => {
  const style = { jitter: 0.3, flip: true, scale: [1, 2] };
  const placement = placeObject({ x: 10, y: 20 }, { width: 2, height: 3 }, style, variants, fnv1a('rock'), 1 / 16);
  assert.equal(placement.variant, 1);
  assert.equal(placement.flip, false);
  assert.ok(Math.abs(placement.foot.x - 10.958777) < 1e-4, `${placement.foot.x}`);
  assert.ok(Math.abs(placement.foot.y - 20.346273) < 1e-4, `${placement.foot.y}`);
  assert.ok(Math.abs(placement.size.width / (10 / 16) - 1.7255) < 1e-4, 'scale');
});

test('snapping puts the art corner on the texel grid, like the client', () => {
  for (let x = 0; x < 40; x++) {
    const style = { jitter: 0.5, flip: true };
    const placement = placeObject({ x, y: 7 }, { width: 1, height: 1 }, style, variants, 3, 1 / 16, 1 / 16);
    const [pivotX, pivotY] = placement.art.pivot ?? [placement.art.rect[2] / 2, placement.art.rect[3]];
    const fromLeft = placement.flip ? placement.art.rect[2] - pivotX : pivotX;
    const corner = [placement.foot.x - fromLeft / 16, placement.foot.y - (placement.art.rect[3] - pivotY) / 16];
    for (const value of corner) assert.ok(Math.abs(value * 16 - Math.round(value * 16)) < 1e-6, `${corner}`);
  }
});

test('placement is deterministic and keeps the foot inside the footprint', () => {
  const seen = new Set();
  for (let x = 0; x < 40; x++) for (let y = 0; y < 40; y++) {
    const a = placeObject({ x, y }, { width: 1, height: 1 }, { jitter: 0.5, flip: true }, variants, 7, 1 / 16);
    const b = placeObject({ x, y }, { width: 1, height: 1 }, { jitter: 0.5, flip: true }, variants, 7, 1 / 16);
    assert.deepEqual(a, b);
    seen.add(a.variant);
    assert.ok(a.foot.x >= x && a.foot.x <= x + 1 && a.foot.y >= y && a.foot.y <= y + 1, JSON.stringify(a.foot));
  }
  assert.equal(seen.size, 3, 'every variant is used');
  assert.equal(styleJitter({ jitter: 3 }), 0.5);
  assert.deepEqual(styleScale({ scale: [2, 0.5] }), [0.5, 2]);
});

test('objects stand on their foot, mirror their pivot, and sort ground first then north to south', () => {
  const look = {
    sheet: {}, tilesPerPx: 1 / 16,
    atlas: { sprites: { rock: [{ rect: [0, 0, 16, 32], pivot: [4, 32] }] } },
    kinds: [
      { name: 'rock', style: {}, salt: 1, placeholder: '#5A5550' },
      { name: 'tree', style: { layer: 'Ground' }, salt: 2, placeholder: '#3E5A34' },
    ],
  };
  const object = (kind, x, y) => ({ kind, id: { anchor: { x, y } }, footprint: { width: 1, height: 1 } });
  const rock = objectVisual(object(0, 3, 5), look, null);
  assert.deepEqual(rock.sprite, { rect: [0, 0, 16, 32], flip: false, width: 1, height: 2, left: 3.5 - 4 / 16, bottom: 5 });
  const mirrored = objectVisual(object(0, 3, 5), { ...look, kinds: [{ ...look.kinds[0], style: { flip: true } }] }, null);
  if (mirrored.sprite.flip) assert.equal(mirrored.sprite.left, 3.5 - 12 / 16, 'a mirrored sprite mirrors its pivot');
  const tree = objectVisual(object(1, 0, 0), look, null);
  assert.equal(tree.color, '#3E5A34', 'kinds without art draw a placeholder');
  const order = sortVisuals([objectVisual(object(0, 0, 1), look, null), tree, objectVisual(object(0, 0, 9), look, null)]);
  assert.deepEqual(order.map(visual => [visual.ground, visual.sortY]), [[true, 0], [false, 9], [false, 1]]);
});
