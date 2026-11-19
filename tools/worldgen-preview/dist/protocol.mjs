// Protocol 7 record: status u8 (0 unclassified, 1 classified, 255 outside the world),
// 3 reserved bytes, height f32, temperature f32, biome u16, ground biome u16 (all LE).
export const PROTOCOL = 7;
export const SAMPLE_BYTES = 16;
export const UNCLASSIFIED_RGB = [54, 43, 61];

export const MAP_MODES = {
  biomes: { label: 'Biomes' },
  temperature: {
    label: 'Temperature', min: -50, max: 50, low: '≤ −50 °C', middle: '0 °C', high: '≥ 50 °C',
    palette: [[40, 70, 160], [75, 170, 215], [240, 245, 235], [245, 170, 65], [190, 45, 45]],
  },
  height: {
    label: 'Height', min: 0, max: 1, low: '0 · Low', middle: '0.5', high: '1 · High',
    palette: [[0, 0, 0], [255, 255, 255]],
  },
};

// View-only coefficients for the flat Biomes view (zoomed out or textures off). They never reach
// the generator or the client; the textured view draws exactly what the client draws.
// Height tint darkens ground below HEIGHT_REFERENCE and brightens ground above it, so crater and
// basin bowls read as dark discs. Relief shades slopes lit from the north-west.
export const HEIGHT_REFERENCE = 0.65;
export const DISPLAY_CONTROLS = [
  { key: 'height_tint', label: 'Height tint', min: 0, max: 10, step: 0.1, default: 3 },
  { key: 'relief', label: 'Relief shading', min: 0, max: 5, step: 0.1, default: 1 },
];
export const DEFAULT_DISPLAY = Object.fromEntries(DISPLAY_CONTROLS.map(control => [control.key, control.default]));
const RELIEF_GAIN = 50;
const RELIEF_LIMIT = 0.4;

export function readDisplay(params) {
  return Object.fromEntries(DISPLAY_CONTROLS.map(({ key, min, max, default: fallback }) => {
    const raw = params.get(key);
    const value = raw === null || raw.trim() === '' ? NaN : Number(raw);
    return [key, Number.isFinite(value) && value >= min && value <= max ? value : fallback];
  }));
}

// slope: (east - west + south - north) / 2 in height units per tile.
export function displayFactor(height, slope, display = DEFAULT_DISPLAY) {
  const tint = Number.isFinite(height) ? Math.max(.25, Math.min(1.75, 1 + display.height_tint * (height - HEIGHT_REFERENCE))) : 1;
  const relief = Number.isFinite(slope) ? 1 + Math.max(-RELIEF_LIMIT, Math.min(RELIEF_LIMIT, slope * RELIEF_GAIN * display.relief)) : 1;
  return tint * relief;
}

export function readMapMode(value) {
  return Object.hasOwn(MAP_MODES, value) ? value : 'biomes';
}

function metricColor(value, mode) {
  if (!Number.isFinite(value)) return [54, 43, 61];
  const position = Math.max(0, Math.min(1, (value - mode.min) / (mode.max - mode.min))) * (mode.palette.length - 1);
  const index = Math.min(Math.floor(position), mode.palette.length - 2);
  const fraction = position - index;
  return mode.palette[index].map((channel, i) => Math.round(channel + (mode.palette[index + 1][i] - channel) * fraction));
}

export function readSample(bytes, index) {
  const offset = index * SAMPLE_BYTES;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const classified = view.getUint8(offset) === 1;
  return {
    status: view.getUint8(offset),
    height: view.getFloat32(offset + 4, true),
    temperature: view.getFloat32(offset + 8, true),
    biomeId: classified ? view.getUint16(offset + 12, true) : null,
    groundId: classified ? view.getUint16(offset + 14, true) : null,
  };
}

const linear = value => { const c = value / 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
const srgb = value => Math.round(Math.min(1, Math.max(0, value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055)) * 255);

// Flat color of a tile: its drawn texture's average color times the drawn biome's tint.
// palette: { own: Uint8Array per biome (1 = own tiles), textureOf: Uint16Array per biome,
// colors: Uint8Array RGB per texture, tints: Uint8Array RGB per biome (optional, white) }.
export function sampleColor(palette, biome, ground) {
  const id = palette.own[biome] ? biome : ground;
  const texture = palette.textureOf[id];
  if (texture === undefined) return UNCLASSIFIED_RGB;
  return [0, 1, 2].map(c => {
    const color = palette.colors[texture * 3 + c];
    const tint = palette.tints ? palette.tints[id * 3 + c] : 255;
    return tint === 255 ? color : srgb(linear(color) * linear(tint));
  });
}

// stride: tiles between neighboring samples (2^LOD), so slopes stay per tile at every zoom.
function sampleSlope(view, bytes, size, index) {
  const x = index % size;
  const y = (index - x) / size;
  const height = i => (bytes[i * SAMPLE_BYTES] === 255 ? NaN : view.getFloat32(i * SAMPLE_BYTES + 4, true));
  const center = height(index);
  // Central differences inside the chunk, one-sided at its edges.
  const axis = (low, high) => {
    const a = low === null ? NaN : height(low);
    const b = high === null ? NaN : height(high);
    if (Number.isFinite(a) && Number.isFinite(b)) return (b - a) / 2;
    if (Number.isFinite(b)) return b - center;
    if (Number.isFinite(a)) return center - a;
    return 0;
  };
  return axis(x > 0 ? index - 1 : null, x < size - 1 ? index + 1 : null)
    + axis(y > 0 ? index - size : null, y < size - 1 ? index + size : null);
}

export function decodePixels(buffer, size, mode = 'biomes', display = DEFAULT_DISPLAY, stride = 1, palette = null) {
  if (buffer.byteLength !== size * size * SAMPLE_BYTES) throw new Error('Incomplete chunk data');
  const bytes = new Uint8Array(buffer);
  const pixels = new Uint8ClampedArray(size * size * 4);
  const view = new DataView(buffer);
  mode = readMapMode(mode);
  for (let i = 0; i < size * size; i++) {
    const offset = i * SAMPLE_BYTES;
    const pixel = i * 4;
    if (bytes[offset] === 255) continue;
    if (mode !== 'biomes') {
      const value = view.getFloat32(offset + (mode === 'height' ? 4 : 8), true);
      pixels.set(metricColor(value, MAP_MODES[mode]), pixel);
    } else if (bytes[offset] === 1 && palette) {
      const factor = displayFactor(view.getFloat32(offset + 4, true), sampleSlope(view, bytes, size, i) / stride, display);
      const rgb = sampleColor(palette, view.getUint16(offset + 12, true), view.getUint16(offset + 14, true));
      pixels[pixel] = rgb[0] * factor;
      pixels[pixel + 1] = rgb[1] * factor;
      pixels[pixel + 2] = rgb[2] * factor;
    } else {
      pixels[pixel] = 54; pixels[pixel + 1] = 43; pixels[pixel + 2] = 61;
    }
    pixels[pixel + 3] = bytes[offset] === 255 ? 0 : 255;
  }
  return pixels;
}
