import { decodePixels, MAP_MODES } from './protocol.mjs';
import { ATLAS_URL } from './sprites.mjs';
import { buildGrid, composeTerrain, terrainTilePx, textureStats } from './terrain.mjs';

let atlasRequest = null;

function loadAtlas() {
  atlasRequest ??= (async () => {
    const response = await fetch(new URL(ATLAS_URL, import.meta.url));
    if (!response.ok) throw new Error(`Texture atlas returned ${response.status}`);
    const bitmap = await createImageBitmap(await response.blob());
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext('2d');
    context.drawImage(bitmap, 0, 0);
    bitmap.close();
    const atlas = context.getImageData(0, 0, canvas.width, canvas.height);
    return { atlas, stats: textureStats(atlas) };
  })();
  atlasRequest.catch(() => { atlasRequest = null; });
  return atlasRequest;
}

async function terrain({ id, neighbors, size, chunkX, chunkY, tiles, display }) {
  try {
    const { atlas, stats } = await loadAtlas();
    const tilePx = terrainTilePx(size);
    const grid = buildGrid(neighbors.map(buffer => buffer && new Uint8Array(buffer)), size, tiles);
    const width = size * tilePx;
    const pixels = composeTerrain({ grid, size, tilePx, chunkX, chunkY, atlas, stats, display });
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
  const { id, buffer, size, display, stride } = data;
  const images = {};
  try {
    const canvas = new OffscreenCanvas(size, size);
    const context = canvas.getContext('2d');
    for (const mode of Object.keys(MAP_MODES)) {
      context.putImageData(new ImageData(decodePixels(buffer, size, mode, display, stride), size, size), 0, 0);
      images[mode] = canvas.transferToImageBitmap();
    }
    self.postMessage({ id, buffer, images }, [buffer, ...Object.values(images)]);
  } catch (error) {
    for (const image of Object.values(images)) image.close();
    self.postMessage({ id, error: String(error) });
  }
};
