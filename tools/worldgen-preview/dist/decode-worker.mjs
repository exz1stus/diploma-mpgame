import { decodePixels, MAP_MODES } from './protocol.mjs';
import { composeChunk } from './terrain.mjs';

// Textured chunk: the client's ground renderer over one chunk's tile data.
function terrain({ id, size, originX, originY, data, hidden, textures, blend }) {
  try {
    const hiddenTiles = hidden && new Uint8Array(hidden);
    const { pixels, width } = composeChunk({
      size, originX, originY, textures, blend,
      data: { layer: new Uint8Array(data.layer), level: new Uint8Array(data.level), tint: new Uint8Array(data.tint) },
      hidden: hiddenTiles ? (x, y) => hiddenTiles[y * size + x] === 1 : undefined,
    });
    const canvas = new OffscreenCanvas(width, width);
    canvas.getContext('2d').putImageData(new ImageData(pixels, width, width), 0, 0);
    const image = canvas.transferToImageBitmap();
    self.postMessage({ id, image }, [image]);
  } catch (error) {
    self.postMessage({ id, error: String(error) });
  }
}

self.onmessage = ({ data }) => {
  if (data.type === 'terrain') { terrain(data); return; }
  const { id, buffer, size, display, stride, palette } = data;
  const images = {};
  try {
    const canvas = new OffscreenCanvas(size, size);
    const context = canvas.getContext('2d');
    for (const mode of Object.keys(MAP_MODES)) {
      context.putImageData(new ImageData(decodePixels(buffer, size, mode, display, stride, palette), size, size), 0, 0);
      images[mode] = canvas.transferToImageBitmap();
    }
    self.postMessage({ id, buffer, images }, [buffer, ...Object.values(images)]);
  } catch (error) {
    for (const image of Object.values(images)) image.close();
    self.postMessage({ id, error: String(error) });
  }
};
