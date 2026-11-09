import test from 'node:test';
import assert from 'node:assert/strict';
import { zoomMetrics } from './dist/zoom.mjs';

test('base tiles remain distinct until their screen size is one pixel', () => {
  const boundary = Math.log2(24);
  assert.equal(zoomMetrics(boundary - 0.01).lod, 0);
  assert.equal(zoomMetrics(boundary).lod, 0);
  assert.ok(Math.abs(zoomMetrics(boundary).baseTilePx - 1) < 1e-12);
  assert.equal(zoomMetrics(boundary + 0.01).lod, 1);
  assert.ok(zoomMetrics(boundary + 0.01).cellPx > 1);
});

test('zooming out continues to larger chunk levels without a fixed cap', () => {
  assert.equal(zoomMetrics(1000).lod, Math.ceil(1000 - Math.log2(24)));
  assert.ok(zoomMetrics(1000).cellPx >= 1);
});
