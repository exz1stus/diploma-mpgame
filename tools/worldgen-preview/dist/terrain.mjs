// Client ground renderer, ported from crates/client/src/world/presentation/terrain.wgsl and
// chunk_mesh.rs. Keep them in sync: the preview must draw exactly what the client draws.
//
// Per pixel, from the four nearest tile centers:
// - height: the highest overlay layer whose coverage (bilinear weight of its tiles and higher
//   ones, plus edge noise) exceeds 0.5 - overlay_reach wins, so higher ground overlays lower
//   ground with a crisp, irregular edge. Lower ground next to such an edge is darkened for a
//   few pixels (ambient occlusion), so steps read as height. The shadow is scaled by
//   ao_south where the higher ground lies north of the pixel (the south side of a step) and
//   by ao_north where it lies south, interpolated in between;
// - biomes: within the winning layer, the biome whose tiles cover the pixel most, plus a smooth
//   noise field of its own, wins: texture and tint change together on a crisp, irregular edge,
//   like height steps but without shading;
// - temperature: the ground is multiplied by the winning biome's tint. Each tile's tint can be
//   averaged over the tiles of its layer in a (2r + 1)² box (tint_radius r) to soften colors
//   inside a biome; with r = 0 every biome is one flat color.
// Positions snap to the texture pixel grid.

export const DEFAULT_BLEND = Object.freeze({
  pixels_per_tile: 16, overlay_reach: 0.15, edge_noise: 0.35, noise_scale: 0.5,
  ao_strength: 0.35, ao_width: 3, ao_south: 1.5, ao_north: 0.4, tint_radius: 0,
});

export const MAX_TINT_RADIUS = 12;

export const BLEND_CONTROLS = [
  { key: 'pixels_per_tile', label: 'Texture pixels per tile', min: 1, max: 256, step: 1 },
  { key: 'overlay_reach', label: 'Overlay reach (tiles)', min: 0, max: 0.5, step: 0.01 },
  { key: 'edge_noise', label: 'Edge noise (height and biome)', min: 0, max: 1, step: 0.01 },
  { key: 'noise_scale', label: 'Edge noise scale (tiles)', min: 0.05, max: 64, step: 0.05 },
  { key: 'ao_strength', label: 'Height shadow strength', min: 0, max: 1, step: 0.05 },
  { key: 'ao_width', label: 'Height shadow width (pixels)', min: 0, max: 16, step: 1 },
  { key: 'ao_south', label: 'Shadow scale, south side', min: 0, max: 4, step: 0.05 },
  { key: 'ao_north', label: 'Shadow scale, north side', min: 0, max: 4, step: 0.05 },
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
    overlay_reach: value('overlay_reach', 0, 0.5),
    edge_noise: value('edge_noise', 0, 1),
    noise_scale: value('noise_scale', 0.05, 64),
    ao_strength: value('ao_strength', 0, 1),
    ao_width: Math.round(value('ao_width', 0, 16)),
    ao_south: value('ao_south', 0, 4),
    ao_north: value('ao_north', 0, 4),
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

// Two octaves in [-0.5, 0.5], continuous in global tile coordinates; `seed` and `seed + 1`.
export function noiseField(px, py, scale, seed) {
  return valueNoise(px / scale, py / scale, seed) * 0.65 + valueNoise(px * 2 / scale, py * 2 / scale, seed + 1) * 0.35 - 0.5;
}

// Height edges.
export function edgeNoise(px, py, scale) {
  return noiseField(px, py, scale, 0);
}

// Each biome's own edge field.
export function biomeNoise(px, py, scale, biome) {
  return noiseField(px, py, scale, 32 + 2 * biome);
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
// [textureLayer, overlayLayer, r, g, b, biome] (r, g, b: the biome tint; biome: the drawn
// biome's ID, which decides biome edges) for chunk-local tiles up to
// one chunk away, or null for tiles of missing neighbors, which repeat the chunk's nearest own
// tile. Returns per data cell (tiles -1..=size): texture layer, overlay layer, biome, and the tint
// averaged over the tiles of the (2r + 1)² box around the tile that share its overlay layer,
// rounded to the nearest integer.
export function buildTileData(size, tileAt, tintRadius = 0) {
  const radius = Math.max(0, Math.min(size - 1, Math.round(tintRadius)));
  const side = dataSide(size);
  const tile = (x, y) => {
    const inside = x >= 0 && y >= 0 && x < size && y < size;
    return (!inside && tileAt(x, y)) || tileAt(Math.min(size - 1, Math.max(0, x)), Math.min(size - 1, Math.max(0, y))) || [0, 0, 255, 255, 255, 0];
  };
  // Every tile a box can reach: data cells plus `radius` around them.
  const span = side + 2 * radius;
  const reach = Array.from({ length: span * span }, (_, index) => tile(index % span - 1 - radius, Math.floor(index / span) - 1 - radius));
  const layer = new Uint8Array(side * side);
  const level = new Uint8Array(side * side);
  const biome = new Uint16Array(side * side);
  const tint = new Uint8Array(side * side * 3);
  for (let cy = 0; cy < side; cy++) for (let cx = 0; cx < side; cx++) {
    const value = tile(cx - 1, cy - 1);
    const index = cy * side + cx;
    layer[index] = Math.min(255, value[0]);
    level[index] = Math.min(255, value[1]);
    biome[index] = Math.min(65535, value[5] ?? 0);
    let r = 0; let g = 0; let b = 0; let count = 0;
    for (let dy = 0; dy <= 2 * radius; dy++) for (let dx = 0; dx <= 2 * radius; dx++) {
      const other = reach[(cy + dy) * span + cx + dx];
      if (Math.min(255, other[1]) !== level[index]) continue;
      r += other[2]; g += other[3]; b += other[4]; count++;
    }
    const half = count >> 1;
    tint[index * 3] = Math.floor((r + half) / count);
    tint[index * 3 + 1] = Math.floor((g + half) / count);
    tint[index * 3 + 2] = Math.floor((b + half) / count);
  }
  return { layer, level, biome, tint };
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
  const { pixels_per_tile: ppt, overlay_reach: reach, edge_noise: amplitude, noise_scale: scale } = settings;
  const { ao_strength: aoStrength, ao_width: aoWidth, ao_south: aoSouth, ao_north: aoNorth } = settings;
  const outPx = imagePixelsPerTile(size, blend);
  const width = size * outPx;
  const side = dataSide(size);
  const { width: tw, height: th, layers } = textures;
  const pixels = new Uint8ClampedArray(width * width * 4);
  const levelOf = new Int32Array(4);
  const biomeOf = new Int32Array(4);
  const cell = new Int32Array(4);
  const coverage = new Float64Array(4);
  for (let py = 0; py < width; py++) {
    const localTileY = Math.floor(py / outPx);
    const texelY = Math.floor((originY + (py + 0.5) / outPx) * ppt);
    const pointY = (texelY + 0.5) / ppt;
    const qy = pointY - 0.5;
    const baseY = Math.floor(qy);
    const fy = qy - baseY;
    const rowY = th - 1 - (((texelY % th) + th) % th);
    const cornerY = baseY - originY + 1;
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
        levelOf[i] = data.level[cell[i]];
        biomeOf[i] = data.biome[cell[i]];
      }
      coverage[0] = (1 - fx) * (1 - fy); coverage[1] = fx * (1 - fy); coverage[2] = (1 - fx) * fy; coverage[3] = fx * fy;

      // Height: the highest covering layer, and how close the nearest higher edge is.
      let chosen = Math.min(levelOf[0], levelOf[1], levelOf[2], levelOf[3]);
      let gap = Infinity;
      let gapLevel = -1;
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
          if (threshold - covered < gap) { gap = threshold - covered; gapLevel = levelOf[i]; }
        }
      }

      // Biome: within the chosen layer, the biome with the most coverage plus its noise.
      let winner = -1;
      let best = -Infinity;
      for (let i = 0; i < 4; i++) {
        if (levelOf[i] !== chosen) continue;
        let first = true;
        for (let j = 0; j < i; j++) if (levelOf[j] === chosen && biomeOf[j] === biomeOf[i]) first = false;
        if (!first) continue;
        let covered = 0;
        for (let j = 0; j < 4; j++) if (levelOf[j] === chosen && biomeOf[j] === biomeOf[i]) covered += coverage[j];
        const score = covered + amplitude * biomeNoise(pointX, pointY, scale, biomeOf[i]);
        if (score > best) { best = score; winner = i; }
      }

      // The winning biome's texture, and its tiles' tints interpolated bilinearly.
      const source = (rowY * tw + (((texelX % tw) + tw) % tw)) * 4;
      const texel = layers[data.layer[cell[winner]]] ?? layers[0];
      let tr = 0; let tg = 0; let tb = 0; let tintTotal = 0;
      for (let i = 0; i < 4; i++) {
        if (levelOf[i] !== chosen || biomeOf[i] !== biomeOf[winner]) continue;
        const weight = Math.max(coverage[i], 1e-6);
        tr += data.tint[cell[i] * 3] * weight;
        tg += data.tint[cell[i] * 3 + 1] * weight;
        tb += data.tint[cell[i] * 3 + 2] * weight;
        tintTotal += weight;
      }
      tr /= tintTotal; tg /= tintTotal; tb /= tintTotal;

      let light = 1;
      if (gapLevel >= 0 && aoWidth > 0) {
        // Which side the higher ground is on: the gradient of its coverage. north = 1 when
        // it lies due north of the pixel (+Y), -1 due south.
        const c = i => (levelOf[i] >= gapLevel ? 1 : 0);
        const gx = (1 - fy) * (c(1) - c(0)) + fy * (c(3) - c(2));
        const gy = (1 - fx) * (c(2) - c(0)) + fx * (c(3) - c(1));
        const length = Math.hypot(gx, gy);
        const north = length > 0 ? gy / length : 0;
        const sideScale = aoNorth + (aoSouth - aoNorth) * (north + 1) / 2;
        // Ambient occlusion in whole-pixel steps: one step per pixel toward the higher edge.
        const shadow = aoWidth * sideScale;
        const steps = Math.ceil(shadow - gap * ppt);
        if (shadow > 0 && steps > 0) light = 1 - Math.min(1, aoStrength * sideScale) * Math.min(shadow, steps) / shadow;
      }
      const out = (py * width + px) * 4;
      pixels[out] = toSrgb(TO_LINEAR[texel[source]] * srgbToLinear(tr / 255) * light);
      pixels[out + 1] = toSrgb(TO_LINEAR[texel[source + 1]] * srgbToLinear(tg / 255) * light);
      pixels[out + 2] = toSrgb(TO_LINEAR[texel[source + 2]] * srgbToLinear(tb / 255) * light);
      pixels[out + 3] = 255;
    }
  }
  return { pixels, width };
}
