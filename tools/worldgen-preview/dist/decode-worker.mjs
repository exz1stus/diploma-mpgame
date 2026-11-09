import { decodePixels, MAP_MODES } from './protocol.mjs';

self.onmessage = ({ data: { id, buffer, size } }) => {
  const images = {};
  try {
    const canvas = new OffscreenCanvas(size, size);
    const context = canvas.getContext('2d');
    for (const mode of Object.keys(MAP_MODES)) {
      context.putImageData(new ImageData(decodePixels(buffer, size, mode), size, size), 0, 0);
      images[mode] = canvas.transferToImageBitmap();
    }
    self.postMessage({ id, buffer, images }, [buffer, ...Object.values(images)]);
  } catch (error) {
    for (const image of Object.values(images)) image.close();
    self.postMessage({ id, error: String(error) });
  }
};
