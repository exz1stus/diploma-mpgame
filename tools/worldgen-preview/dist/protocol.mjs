export const SAMPLE_BYTES = 12;

export function readSample(bytes, index) {
  const offset = index * SAMPLE_BYTES;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    status: view.getUint8(offset),
    color: `#${[1, 2, 3].map(channel => view.getUint8(offset + channel).toString(16).padStart(2, '0')).join('')}`,
    height: view.getFloat32(offset + 4, true),
    temperature: view.getFloat32(offset + 8, true),
  };
}

export function decodePixels(buffer, size) {
  if (buffer.byteLength !== size * size * SAMPLE_BYTES) throw new Error('Incomplete chunk data');
  const bytes = new Uint8Array(buffer);
  const pixels = new Uint8ClampedArray(size * size * 4);
  for (let i = 0; i < size * size; i++) {
    const offset = i * SAMPLE_BYTES;
    const pixel = i * 4;
    if (bytes[offset] === 1) {
      pixels[pixel] = bytes[offset + 1];
      pixels[pixel + 1] = bytes[offset + 2];
      pixels[pixel + 2] = bytes[offset + 3];
    } else if (bytes[offset] === 0) {
      pixels[pixel] = 54; pixels[pixel + 1] = 43; pixels[pixel + 2] = 61;
    }
    pixels[pixel + 3] = bytes[offset] === 255 ? 0 : 255;
  }
  return pixels;
}
