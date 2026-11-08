import test from 'node:test';
import assert from 'node:assert/strict';
import { collectObjects, objectAt, objectKey, objectStyle, objectsVisibleAt, readObjectChunk } from './dist/objects.mjs';
import { zoomMetrics } from './dist/zoom.mjs';

const types = [
  { name: 'large_rock', footprint: { width: 2, height: 2 } },
  { name: 'tree', footprint: { width: 1, height: 1 } },
];
const crossing = { id: { rule_id: 12, anchor: { x: 31, y: 31 } }, kind: 0, footprint: { width: 2, height: 2 } };
const tree = { id: { rule_id: 13, anchor: { x: 34, y: 32 } }, kind: 1, footprint: { width: 1, height: 1 } };

test('crossing objects merge by global instance ID and stay visible with an outside anchor', () => {
  const copies = [{ objects: [crossing] }, { objects: [structuredClone(crossing), tree] }];
  const visible = collectObjects(copies, { minX: 32, minY: 32, maxX: 40, maxY: 40 });
  assert.equal(visible.length, 2);
  assert.equal(objectKey(visible[0]), '12:31:31');
  assert.equal(objectAt(visible, 32, 32), visible[0]);
  assert.equal(objectAt(visible, 33, 32), null, 'footprint upper bounds are exclusive');
  assert.equal(objectAt(visible, 34, 32), tree);
  assert.equal(collectObjects(copies, { minX: 33, minY: 33, maxX: 34, maxY: 34 }).length, 0);
  assert.equal(collectObjects([{ objects: [crossing, { ...crossing, id: { ...crossing.id, rule_id: 14 } }] }],
    { minX: 30, minY: 30, maxX: 33, maxY: 33 }).length, 2, 'rule IDs are part of instance identity');
});

test('object chunk validation accepts external anchors and discards duplicate ground data', () => {
  const chunk = { position: { x: 1, y: 1 }, size: 32, tiles: Array(1024).fill(0), objects: [crossing] };
  const world = { width: 128, height: 128 };
  assert.deepEqual(readObjectChunk(chunk, 1, 1, 32, types, world), { objects: [crossing] });
  for (const broken of [
    { ...chunk, position: { x: 0, y: 1 } },
    { ...chunk, size: 16 },
    { ...chunk, objects: [{ ...crossing, kind: 99 }] },
    { ...chunk, objects: [{ ...crossing, footprint: { width: 0, height: 2 } }] },
    { ...chunk, objects: [{ ...crossing, footprint: { width: 3, height: 2 } }] },
    { ...chunk, objects: [{ ...crossing, id: { rule_id: -1, anchor: crossing.id.anchor } }] },
    { ...chunk, objects: [{ ...crossing, id: { rule_id: 12, anchor: { x: -1, y: 32 } } }] },
    { ...chunk, objects: [{ ...crossing, id: { rule_id: 12, anchor: { x: 10, y: 10 } } }] },
  ]) assert.throws(() => readObjectChunk(broken, 1, 1, 32, types, world));
  assert.throws(() => readObjectChunk(chunk, 1, 1, 32, types, { width: 32, height: 32 }));
});

test('overview zooms suppress placement requests and symbols have a fallback for new types', () => {
  assert.equal(objectsVisibleAt(zoomMetrics(0)), true);
  assert.equal(objectsVisibleAt({ lod: 0, baseTilePx: 4 }), true);
  assert.equal(objectsVisibleAt({ lod: 0, baseTilePx: 3 }), false);
  assert.equal(objectsVisibleAt({ lod: 1, baseTilePx: 12 }), false);
  assert.equal(objectsVisibleAt(zoomMetrics(8)), false);
  assert.equal(objectStyle(types[1]).label, 'Tree');
  assert.equal(objectStyle({ name: 'new_object' }).label, 'new object');
  assert.equal(objectStyle({ name: 'constructor' }).label, 'constructor');
});
