export function zoomMetrics(zoom, initialTilePixels = 24) {
  const baseTilePx = initialTilePixels * 2 ** -zoom;
  // Keep every base tile distinct down to exactly one screen pixel.
  const lod = Math.max(0, Math.ceil(zoom - Math.log2(initialTilePixels)));
  return { lod, baseTilePx, cellPx: initialTilePixels * 2 ** (lod - zoom) };
}

export function fitWorldZoom(world, width, height) {
  return Math.log2(24 * Math.max(world.width / Math.max(1, width), world.height / Math.max(1, height)));
}

export function clampCamera(x, y, metrics, width, height, world) {
  const stride = 2 ** metrics.lod;
  function axis(value, extent, viewport) {
    const cells = extent / stride;
    const halfView = viewport / (2 * metrics.cellPx);
    return cells <= 2 * halfView ? cells / 2 : Math.min(cells - halfView, Math.max(halfView, value));
  }
  return { x: axis(x, world.width, width), y: axis(y, world.height, height) };
}
