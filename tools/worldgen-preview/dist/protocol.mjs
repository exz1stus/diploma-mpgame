export const SAMPLE_BYTES = 18;

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
  return {
    status: view.getUint8(offset),
    color: `#${[1, 2, 3].map(channel => view.getUint8(offset + channel).toString(16).padStart(2, '0')).join('')}`,
    height: view.getFloat32(offset + 4, true),
    temperature: view.getFloat32(offset + 8, true),
    biomeId: [1, 2].includes(view.getUint8(offset)) ? view.getUint16(offset + 12, true) : null,
    surfaceBiomeId: view.getUint8(offset) === 1 ? view.getUint16(offset + 14, true) : null,
    paletteIndex: view.getUint8(offset) === 1 ? view.getUint16(offset + 16, true) : null,
  };
}

export function decodePixels(buffer, size, mode = 'biomes') {
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
    } else if (bytes[offset] === 1) {
      pixels[pixel] = bytes[offset + 1];
      pixels[pixel + 1] = bytes[offset + 2];
      pixels[pixel + 2] = bytes[offset + 3];
    } else {
      pixels[pixel] = 54; pixels[pixel + 1] = 43; pixels[pixel + 2] = 61;
    }
    pixels[pixel + 3] = bytes[offset] === 255 ? 0 : 255;
  }
  return pixels;
}
