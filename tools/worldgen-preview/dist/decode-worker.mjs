import { decodePixels } from './protocol.mjs';

self.onmessage = ({ data: { id, buffer, size } }) => {
  try {
    const pixels = decodePixels(buffer, size);
    const canvas = new OffscreenCanvas(size, size);
    canvas.getContext('2d').putImageData(new ImageData(pixels, size, size), 0, 0);
    const image = canvas.transferToImageBitmap();
    self.postMessage({ id, buffer, image }, [buffer, image]);
  } catch (error) {
    self.postMessage({ id, error: String(error) });
  }
};
