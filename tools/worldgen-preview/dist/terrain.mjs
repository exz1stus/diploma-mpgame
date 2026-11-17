import { DEFAULT_DISPLAY, SAMPLE_BYTES, displayFactor } from './protocol.mjs';
import { PEBBLE_CELLS, SOURCE_TILE_PX, SURFACE, SURFACE_TEXTURES, TEXTURE_PX, pebbleChance, surfaceFor } from './sprites.mjs';

export const UNCLASSIFIED_TINT = [54, 43, 61];
const EDGE_NOISE = 0.35;
// Sampling positions are displaced by up to WARP tiles, so tile-aligned biome borders become
// organic curves. MARGIN covers the displaced bilinear footprint plus the central height
// differences used for relief shading, so neighboring chunk images meet without seams.
const WARP = 0.45;
export const MARGIN = 3;

// Pixels per tile in composed chunk images. Larger chunks halve it to bound bitmap memory.
export function terrainTilePx(size) {
  return size <= 64 ? SOURCE_TILE_PX : SOURCE_TILE_PX / 2;
}

export function textureStats(atlas) {
  const stats = SURFACE_TEXTURES.map(({ origin: [ox, oy] }) => {
    const mean = [0, 0, 0];
    for (let y = 0; y < TEXTURE_PX; y++) for (let x = 0; x < TEXTURE_PX; x++) {
      const offset = ((oy + y) * atlas.width + ox + x) * 4;
      for (let c = 0; c < 3; c++) mean[c] += atlas.data[offset + c];
    }
    return mean.map(value => value / TEXTURE_PX ** 2);
  });
  const gray = SURFACE_TEXTURES.map((texture, i) => texture.tint === 'luminance' ? stats[i][0] : null).filter(v => v !== null);
  const grayMean = gray.reduce((sum, value) => sum + value, 0) / gray.length;
  // Darker gray variants stay slightly darker after tinting, keeping their character.
  const surfaces = stats.map((mean, i) => ({ mean, brightness: SURFACE_TEXTURES[i].tint === 'luminance' ? Math.sqrt(mean[0] / grayMean) : 1 }));
  let pebbleSum = 0;
  let pebbleCount = 0;
  for (const [ox, oy] of PEBBLE_CELLS) for (let y = 0; y < TEXTURE_PX; y++) for (let x = 0; x < TEXTURE_PX; x++) {
    const offset = ((oy + y) * atlas.width + ox + x) * 4;
    if (atlas.data[offset + 3] === 255) { pebbleSum += atlas.data[offset]; pebbleCount++; }
  }
  surfaces.pebbleMean = pebbleSum / Math.max(1, pebbleCount);
  return surfaces;
}

// neighbors: 9 sample buffers in row-major order around the chunk (index 4 is the chunk itself).
// Missing neighbors repeat the chunk edge; outside-world padding stays transparent.
export function buildGrid(neighbors, size, tiles) {
  const span = size + 2 * MARGIN;
  const surface = new Int8Array(span * span).fill(-1);
  const tint = new Float32Array(span * span * 3);
  const height = new Float32Array(span * span).fill(NaN);
  for (let gy = 0; gy < span; gy++) for (let gx = 0; gx < span; gx++) {
    const y = gy - MARGIN;
    const x = gx - MARGIN;
    const ny = y < 0 ? 0 : y >= size ? 2 : 1;
    const nx = x < 0 ? 0 : x >= size ? 2 : 1;
    let bytes = neighbors[ny * 3 + nx];
    let localX = (x + size) % size;
    let localY = (y + size) % size;
    if (!bytes) { bytes = neighbors[4]; localX = Math.min(size - 1, Math.max(0, x)); localY = Math.min(size - 1, Math.max(0, y)); }
    const offset = (localY * size + localX) * SAMPLE_BYTES;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const status = bytes[offset];
    if (status === 255) continue;
    const tile = status === 1 ? tiles[view.getUint16(offset + 14, true)] : null;
    const color = tile ? tile.tint : UNCLASSIFIED_TINT;
    const value = view.getFloat32(offset + 4, true);
    const temperature = view.getFloat32(offset + 8, true);
    const cell = gy * span + gx;
    height[cell] = Number.isFinite(value) ? value : 0.5;
    surface[cell] = surfaceFor(!!tile?.basalt, height[cell], Number.isFinite(temperature) ? temperature : 0);
    tint.set(color, cell * 3);
  }
  // Per-tile slopes (central differences), interpolated per pixel for smooth relief shading.
  const slope = new Float32Array(span * span);
  const at = (cell, fallback) => Number.isNaN(height[cell]) ? fallback : height[cell];
  for (let gy = 1; gy < span - 1; gy++) for (let gx = 1; gx < span - 1; gx++) {
    const cell = gy * span + gx;
    const h = height[cell];
    if (Number.isNaN(h)) continue;
    slope[cell] = (at(cell + 1, h) - at(cell - 1, h) + at(cell + span, h) - at(cell - span, h)) / 2;
  }
  return { surface, tint, height, slope };
}

function lattice(x, y, seed) {
  let h = Math.imul(x, 0x27d4eb2d) ^ Math.imul(y, 0x165667b1) ^ Math.imul(seed + 1, 0x9e3779b1);
  h = Math.imul(h ^ (h >>> 15), 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

function valueNoise(x, y, cell, seed) {
  const fx = x / cell;
  const fy = y / cell;
  const ix = Math.floor(fx);
  const iy = Math.floor(fy);
  let u = fx - ix;
  let v = fy - iy;
  u = u * u * (3 - 2 * u);
  v = v * v * (3 - 2 * v);
  const top = lattice(ix, iy, seed) * (1 - u) + lattice(ix + 1, iy, seed) * u;
  const bottom = lattice(ix, iy + 1, seed) * (1 - u) + lattice(ix + 1, iy + 1, seed) * u;
  return top * (1 - v) + bottom * v;
}

// Global source-pixel coordinates keep edge noise continuous across chunk images.
export function edgeNoise(x, y, surface) {
  return valueNoise(x, y, 7, surface) * .65 + valueNoise(x, y, 3, surface + 17) * .35 - .5;
}

export function warpOffset(x, y, seed) {
  return ((valueNoise(x, y, 22, seed) - .5) * 2 + (valueNoise(x, y, 6, seed + 1) - .5) * .5) * WARP;
}

// Pebble cells sit in 32 px blocks with a random offset, flip, and variant per block. A pixel
// can be covered by its own block or the blocks above/left of it. Returns the atlas offset of
// the covering opaque pebble pixel, or -1.
function pebbleAt(atlas, x, y, chance) {
  if (chance <= 0) return -1;
  const bx = Math.floor(x / TEXTURE_PX);
  const by = Math.floor(y / TEXTURE_PX);
  for (let dy = 0; dy >= -1; dy--) for (let dx = 0; dx >= -1; dx--) {
    const cx = bx + dx;
    const cy = by + dy;
    if (lattice(cx, cy, 211) >= chance) continue;
    const bits = (lattice(cx, cy, 223) * 4294967296) >>> 0;
    let lx = x - cx * TEXTURE_PX - (bits & 15);
    let ly = y - cy * TEXTURE_PX - ((bits >>> 4) & 15);
    if (lx < 0 || ly < 0 || lx >= TEXTURE_PX || ly >= TEXTURE_PX) continue;
    if (bits & 256) lx = TEXTURE_PX - 1 - lx;
    const [ox, oy] = PEBBLE_CELLS[(bits >>> 9) % PEBBLE_CELLS.length];
    const offset = ((oy + ly) * atlas.width + ox + lx) * 4;
    if (atlas.data[offset + 3] > 0) return offset;
  }
  return -1;
}

function axis(size, tilePx, origin) {
  const count = size * tilePx;
  const position = new Float32Array(count);
  const texel = new Int32Array(count);
  const local = new Int32Array(count);
  for (let p = 0; p < count; p++) {
    const tile = Math.floor(p / tilePx);
    position[p] = tile + (p - tile * tilePx + .5) / tilePx - .5;
    texel[p] = Math.floor((origin * tilePx + p) * SOURCE_TILE_PX / tilePx);
    local[p] = tile;
  }
  return { position, texel, local };
}

// Bilinear weights over the four nearest (warped) tile centers give smooth tint gradients.
// Materials switch per pixel where weight plus edge noise is largest, giving pixel-art borders.
export function composeTerrain({ grid, size, tilePx, chunkX, chunkY, atlas, stats, display = DEFAULT_DISPLAY }) {
  const span = size + 2 * MARGIN;
  const width = size * tilePx;
  const pixels = new Uint8ClampedArray(width * width * 4);
  const xs = axis(size, tilePx, chunkX * size);
  const ys = axis(size, tilePx, chunkY * size);
  const { surface, tint, height, slope } = grid;
  const cells = new Int32Array(4);
  const weights = new Float32Array(4);
  const candidates = new Int8Array(4);
  const totals = new Float32Array(4);
  for (let py = 0; py < width; py++) {
    const centerRow = (ys.local[py] + MARGIN) * span;
    const texelY = ys.texel[py];
    for (let px = 0; px < width; px++) {
      if (surface[centerRow + xs.local[px] + MARGIN] < 0) continue;
      const texelX = xs.texel[px];
      const u = xs.position[px] + warpOffset(texelX, texelY, 41);
      const v = ys.position[py] + warpOffset(texelX, texelY, 73);
      const fx = Math.floor(u);
      const fy = Math.floor(v);
      const wx = u - fx;
      const wy = v - fy;
      const base = (fy + MARGIN) * span + fx + MARGIN;
      cells[0] = base; cells[1] = base + 1; cells[2] = base + span; cells[3] = base + span + 1;
      weights[0] = (1 - wx) * (1 - wy); weights[1] = wx * (1 - wy); weights[2] = (1 - wx) * wy; weights[3] = wx * wy;
      let sum = 0; let r = 0; let g = 0; let b = 0; let h = 0; let light = 0; let chosen = -1; let mixed = false;
      for (let i = 0; i < 4; i++) {
        const s = surface[cells[i]];
        if (s < 0) { weights[i] = 0; continue; }
        const w = weights[i];
        sum += w; r += tint[cells[i] * 3] * w; g += tint[cells[i] * 3 + 1] * w; b += tint[cells[i] * 3 + 2] * w;
        h += height[cells[i]] * w; light += slope[cells[i]] * w;
        if (chosen < 0) chosen = s; else if (s !== chosen) mixed = true;
      }
      if (sum === 0) {
        // Warped samples can land entirely in outside-world padding; use the tile itself.
        const cell = centerRow + xs.local[px] + MARGIN;
        cells[0] = cell; weights[0] = sum = 1; weights[1] = weights[2] = weights[3] = 0;
        chosen = surface[cell]; mixed = false;
        r = tint[cell * 3]; g = tint[cell * 3 + 1]; b = tint[cell * 3 + 2];
        h = height[cell]; light = slope[cell];
      }
      r /= sum; g /= sum; b /= sum; h /= sum;
      light = displayFactor(h, light / sum, display);
      if (mixed) {
        let count = 0;
        for (let i = 0; i < 4; i++) {
          if (weights[i] === 0) continue;
          const s = surface[cells[i]];
          let slot = 0;
          while (slot < count && candidates[slot] !== s) slot++;
          if (slot === count) { candidates[count] = s; totals[count++] = 0; }
          totals[slot] += weights[i] / sum;
        }
        let best = -Infinity;
        for (let i = 0; i < count; i++) {
          const score = totals[i] + EDGE_NOISE * edgeNoise(texelX, texelY, candidates[i]);
          if (score > best) { best = score; chosen = candidates[i]; }
        }
      }
      const texture = SURFACE_TEXTURES[chosen];
      const { mean, brightness } = stats[chosen];
      const source = ((texture.origin[1] + (texelY & (TEXTURE_PX - 1))) * atlas.width + texture.origin[0] + (texelX & (TEXTURE_PX - 1))) * 4;
      const out = (py * width + px) * 4;
      let red; let green; let blue;
      if (texture.tint === 'luminance') {
        const k = atlas.data[source] / mean[0] * brightness;
        red = r * k; green = g * k; blue = b * k;
      } else {
        const s = texture.strength;
        red = atlas.data[source] * (1 + s * (r / mean[0] - 1));
        green = atlas.data[source + 1] * (1 + s * (g / mean[1] - 1));
        blue = atlas.data[source + 2] * (1 + s * (b / mean[2] - 1));
      }
      if (chosen !== SURFACE.ICE && chosen !== SURFACE.SNOW) {
        // Pebbles are gray art tinted like the ground; their outlines keep them readable.
        const pebble = pebbleAt(atlas, texelX, texelY, pebbleChance(h));
        if (pebble >= 0) {
          const alpha = atlas.data[pebble + 3] / 255;
          const k = atlas.data[pebble] / stats.pebbleMean * .85;
          red += (r * k - red) * alpha; green += (g * k - green) * alpha; blue += (b * k - blue) * alpha;
        }
      }
      pixels[out] = red * light; pixels[out + 1] = green * light; pixels[out + 2] = blue * light;
      pixels[out + 3] = 255;
    }
  }
  return pixels;
}
