// Client ground renderer, ported from crates/client/src/world_presentation/terrain.wgsl and
// chunk_mesh.rs. Keep them in sync: the preview must draw exactly what the client draws.
//
// Per pixel, from the four nearest tile centers:
// - height: the highest overlay layer whose coverage (bilinear weight of its tiles and higher
//   ones, plus edge noise) exceeds 0.5 - overlay_reach wins, so higher ground overlays lower
//   ground with a crisp, irregular edge. Lower ground next to such an edge is darkened for a
//   few pixels (ambient occlusion), so steps read as height;
// - textures of the winning layer blend bilinearly (smooth_width wide);
// - temperature: the ground is multiplied by a tint. Each tile's tint is its biome's color
//   averaged over a (2r + 1)² box of tiles (tint_radius r), and pixels interpolate those
//   bilinearly, so biome colors change in very smooth gradients, as in Minecraft.
// Positions snap to the texture pixel grid.

export const DEFAULT_BLEND = Object.freeze({
  pixels_per_tile: 16, smooth_width: 1, overlay_reach: 0.15, edge_noise: 0.35, noise_scale: 0.5,
  ao_strength: 0.35, ao_width: 3, tint_radius: 5,
});

export const MAX_TINT_RADIUS = 12;

export const BLEND_CONTROLS = [
  { key: 'pixels_per_tile', label: 'Texture pixels per tile', min: 1, max: 256, step: 1 },
  { key: 'smooth_width', label: 'Same-layer blend width (tiles)', min: 0, max: 1, step: 0.05 },
  { key: 'overlay_reach', label: 'Overlay reach (tiles)', min: 0, max: 0.5, step: 0.01 },
  { key: 'edge_noise', label: 'Overlay edge noise', min: 0, max: 1, step: 0.01 },
  { key: 'noise_scale', label: 'Edge noise scale (tiles)', min: 0.05, max: 64, step: 0.05 },
  { key: 'ao_strength', label: 'Height shadow strength', min: 0, max: 1, step: 0.05 },
  { key: 'ao_width', label: 'Height shadow width (pixels)', min: 0, max: 16, step: 1 },
  { key: 'tint_radius', label: 'Tint smoothing radius (tiles)', min: 0, max: MAX_TINT_RADIUS, step: 1 },
];

// Same clamping as TileBlend::sanitized in the client.
export function sanitizeBlend(blend = {}) {
  const value = (key, min, max) => {
    const raw = Number(blend[key]);
    return Number.isFinite(raw) ? Math.min(max, Math.max(min, raw)) : DEFAULT_BLEND[key];
  };
  return {
    pixels_per_tile: Math.round(value('pixels_per_tile', 1, 256)),
    smooth_width: value('smooth_width', 0, 1),
    overlay_reach: value('overlay_reach', 0, 0.5),
    edge_noise: value('edge_noise', 0, 1),
    noise_scale: value('noise_scale', 0.05, 64),
    ao_strength: value('ao_strength', 0, 1),
    ao_width: Math.round(value('ao_width', 0, 16)),
    tint_radius: Math.round(value('tint_radius', 0, MAX_TINT_RADIUS)),
  };
}

export function lattice(x, y, seed) {
  let h = Math.imul(x, 0x27d4eb2d) ^ Math.imul(y, 0x165667b1) ^ Math.imul(seed + 1, 0x9e3779b1);
  h = Math.imul(h ^ (h >>> 15), 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

function valueNoise(px, py, seed) {
  const cx = Math.floor(px);
  const cy = Math.floor(py);
  let u = px - cx;
  let v = py - cy;
  u = u * u * (3 - 2 * u);
  v = v * v * (3 - 2 * v);
  const bottom = lattice(cx, cy, seed) + (lattice(cx + 1, cy, seed) - lattice(cx, cy, seed)) * u;
  const top = lattice(cx, cy + 1, seed) + (lattice(cx + 1, cy + 1, seed) - lattice(cx, cy + 1, seed)) * u;
  return bottom + (top - bottom) * v;
}

// In [-0.5, 0.5], continuous in global tile coordinates.
export function edgeNoise(px, py, scale) {
  return valueNoise(px / scale, py / scale, 0) * 0.65 + valueNoise(px * 2 / scale, py * 2 / scale, 1) * 0.35 - 0.5;
}

function smoothAxis(f, width) {
  return width <= 0 ? (f >= 0.5 ? 1 : 0) : Math.min(1, Math.max(0, (f - 0.5) / width + 0.5));
}

export function srgbToLinear(c) {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

const TO_LINEAR = Float32Array.from({ length: 256 }, (_, i) => srgbToLinear(i / 255));

function toSrgb(linear) {
  const c = linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055;
  return Math.round(Math.min(1, Math.max(0, c)) * 255);
}

// Side of a chunk's tile data grid: one-tile border from the neighbors, as in the client.
export function dataSide(size) { return size + 2; }

// Tile data for one chunk, like chunk_mesh::tile_data in the client. `tileAt(x, y)` gives
// [textureLayer, overlayLayer, r, g, b] (r, g, b: the biome tint) for chunk-local tiles up to
// one chunk away, or null for tiles of missing neighbors, which repeat the chunk's nearest own
// tile. Returns per data cell (tiles -1..=size): texture layer, overlay layer, and the tint
// averaged over the (2r + 1)² box around the tile, rounded to the nearest integer.
export function buildTileData(size, tileAt, tintRadius = 0) {
  const radius = Math.max(0, Math.min(size - 1, Math.round(tintRadius)));
  const side = dataSide(size);
  const tile = (x, y) => {
    const inside = x >= 0 && y >= 0 && x < size && y < size;
    return (!inside && tileAt(x, y)) || tileAt(Math.min(size - 1, Math.max(0, x)), Math.min(size - 1, Math.max(0, y))) || [0, 0, 255, 255, 255];
  };
  const layer = new Uint8Array(side * side);
  const level = new Uint8Array(side * side);
  const tint = new Uint8Array(side * side * 3);
  // Exact integer box sums: horizontal over the rows the vertical pass needs, then vertical.
  const span = side + 2 * radius;
  const rows = new Uint32Array(span * side * 3);
  for (let ry = 0; ry < span; ry++) {
    const y = ry - 1 - radius;
    for (let cx = 0; cx < side; cx++) {
      const x = cx - 1;
      let r = 0; let g = 0; let b = 0;
      for (let dx = -radius; dx <= radius; dx++) {
        const value = tile(x + dx, y);
        r += value[2]; g += value[3]; b += value[4];
      }
      rows.set([r, g, b], (ry * side + cx) * 3);
    }
  }
  const count = (2 * radius + 1) ** 2;
  for (let cy = 0; cy < side; cy++) for (let cx = 0; cx < side; cx++) {
    const value = tile(cx - 1, cy - 1);
    const index = cy * side + cx;
    layer[index] = Math.min(255, value[0]);
    level[index] = Math.min(255, value[1]);
    for (let c = 0; c < 3; c++) {
      let sum = 0;
      for (let dy = 0; dy <= 2 * radius; dy++) sum += rows[((cy + dy) * side + cx) * 3 + c];
      tint[index * 3 + c] = Math.floor((sum + (count >> 1)) / count);
    }
  }
  return { layer, level, tint };
}

// Pixels per tile of a composed chunk image: the texture resolution, capped so an image
// stays at most 1024 px wide. Lower resolutions sample the texture like GPU minification.
export function imagePixelsPerTile(size, blend) {
  return Math.max(1, Math.min(sanitizeBlend(blend).pixels_per_tile, Math.floor(1024 / size)));
}

// textures: { width, height, layers: [Uint8Array RGBA] }. Rows of the returned RGBA image
// run from the chunk's bottom (row 0 = local y 0), matching the preview's y-up drawing.
// `hidden(x, y)` marks chunk tiles outside the world; they stay transparent.
export function composeChunk({ size, originX, originY, data, textures, blend, hidden = () => false }) {
  const settings = sanitizeBlend(blend);
  const { pixels_per_tile: ppt, smooth_width: smoothWidth, overlay_reach: reach, edge_noise: amplitude, noise_scale: scale } = settings;
  const { ao_strength: aoStrength, ao_width: aoWidth } = settings;
  const outPx = imagePixelsPerTile(size, blend);
  const width = size * outPx;
  const side = dataSide(size);
  const { width: tw, height: th, layers } = textures;
  const pixels = new Uint8ClampedArray(width * width * 4);
  const layerOf = new Int32Array(4);
  const levelOf = new Int32Array(4);
  const cell = new Int32Array(4);
  const coverage = new Float64Array(4);
  const fade = new Float64Array(4);
  for (let py = 0; py < width; py++) {
    const localTileY = Math.floor(py / outPx);
    const texelY = Math.floor((originY + (py + 0.5) / outPx) * ppt);
    const pointY = (texelY + 0.5) / ppt;
    const qy = pointY - 0.5;
    const baseY = Math.floor(qy);
    const fy = qy - baseY;
    const rowY = th - 1 - (((texelY % th) + th) % th);
    const cornerY = baseY - originY + 1;
    const fadeY = smoothAxis(fy, smoothWidth);
    for (let px = 0; px < width; px++) {
      if (hidden(Math.floor(px / outPx), localTileY)) continue;
      const texelX = Math.floor((originX + (px + 0.5) / outPx) * ppt);
      const pointX = (texelX + 0.5) / ppt;
      const qx = pointX - 0.5;
      const baseX = Math.floor(qx);
      const fx = qx - baseX;
      const cornerX = baseX - originX + 1;
      for (let i = 0; i < 4; i++) {
        const x = Math.min(side - 1, Math.max(0, cornerX + (i & 1)));
        const y = Math.min(side - 1, Math.max(0, cornerY + (i >> 1)));
        cell[i] = y * side + x;
        layerOf[i] = data.layer[cell[i]];
        levelOf[i] = data.level[cell[i]];
      }
      coverage[0] = (1 - fx) * (1 - fy); coverage[1] = fx * (1 - fy); coverage[2] = (1 - fx) * fy; coverage[3] = fx * fy;
      const fadeX = smoothAxis(fx, smoothWidth);
      fade[0] = (1 - fadeX) * (1 - fadeY); fade[1] = fadeX * (1 - fadeY); fade[2] = (1 - fadeX) * fadeY; fade[3] = fadeX * fadeY;

      // Height: the highest covering layer, and how close the nearest higher edge is.
      let chosen = Math.min(levelOf[0], levelOf[1], levelOf[2], levelOf[3]);
      let gap = Infinity;
      if (levelOf[0] !== chosen || levelOf[1] !== chosen || levelOf[2] !== chosen || levelOf[3] !== chosen) {
        const threshold = 0.5 - reach - amplitude * edgeNoise(pointX, pointY, scale);
        for (let i = 0; i < 4; i++) {
          const level = levelOf[i];
          if (level <= chosen) continue;
          let covered = 0;
          for (let j = 0; j < 4; j++) if (levelOf[j] >= level) covered += coverage[j];
          if (covered > threshold) chosen = level;
        }
        for (let i = 0; i < 4; i++) {
          if (levelOf[i] <= chosen) continue;
          let covered = 0;
          for (let j = 0; j < 4; j++) if (levelOf[j] >= levelOf[i]) covered += coverage[j];
          gap = Math.min(gap, threshold - covered);
        }
      }

      // Textures of the chosen layer, blended bilinearly.
      const source = (rowY * tw + (((texelX % tw) + tw) % tw)) * 4;
      let total = 0;
      let r = 0; let g = 0; let b = 0;
      let fallback = 0;
      let strongest = -1;
      for (let i = 0; i < 4; i++) {
        if (levelOf[i] !== chosen) continue;
        if (coverage[i] > strongest) { strongest = coverage[i]; fallback = i; }
        if (fade[i] <= 0) continue;
        const texel = layers[layerOf[i]] ?? layers[0];
        r += TO_LINEAR[texel[source]] * fade[i];
        g += TO_LINEAR[texel[source + 1]] * fade[i];
        b += TO_LINEAR[texel[source + 2]] * fade[i];
        total += fade[i];
      }
      if (total <= 0) {
        const texel = layers[layerOf[fallback]] ?? layers[0];
        r = TO_LINEAR[texel[source]]; g = TO_LINEAR[texel[source + 1]]; b = TO_LINEAR[texel[source + 2]];
      } else {
        r /= total; g /= total; b /= total;
      }

      // Temperature tint: the smoothed tile tints, interpolated bilinearly.
      let tr = 0; let tg = 0; let tb = 0;
      for (let i = 0; i < 4; i++) {
        tr += data.tint[cell[i] * 3] * coverage[i];
        tg += data.tint[cell[i] * 3 + 1] * coverage[i];
        tb += data.tint[cell[i] * 3 + 2] * coverage[i];
      }
      let light = 1;
      if (gap < Infinity && aoWidth > 0) {
        // Ambient occlusion in whole-pixel steps: one step per pixel toward the higher edge.
        const steps = Math.ceil(aoWidth - gap * ppt);
        if (steps > 0) light = 1 - aoStrength * Math.min(aoWidth, steps) / aoWidth;
      }
      const out = (py * width + px) * 4;
      pixels[out] = toSrgb(r * srgbToLinear(tr / 255) * light);
      pixels[out + 1] = toSrgb(g * srgbToLinear(tg / 255) * light);
      pixels[out + 2] = toSrgb(b * srgbToLinear(tb / 255) * light);
      pixels[out + 3] = 255;
    }
  }
  return { pixels, width };
}
