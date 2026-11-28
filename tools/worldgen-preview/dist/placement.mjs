// Port of crates/client/src/world/presentation/placement.rs: where and how an object's sprite
// is drawn, decided from its anchor tile alone. Keep the two in sync; placement.test.mjs
// asserts the same hash vectors as the Rust tests. Positions here are in tiles.

const CHANNEL = { variant: 0, jitterX: 1, jitterY: 2, flip: 3, scale: 4, ore: 5 };

const channelHash = (anchor, salt, channel) =>
  hash32(anchor.x >>> 0, anchor.y >>> 0, (salt ^ Math.imul(channel, 0x9e3779b9)) >>> 0);

// pick_ore: which of `count` ores the object anchored at `anchor` is (0 when there are none).
export const pickOre = (anchor, salt, count) => channelHash(anchor, salt, CHANNEL.ore) % Math.max(1, count);

// 32-bit hash of a tile and a salt (lowbias32 finalizer), in wrapping u32 math.
export function hash32(x, y, salt) {
  let h = (Math.imul(x, 0x27d4eb2d) ^ Math.imul(y, 0x165667b1) ^ salt) >>> 0;
  h = (h ^ (h >>> 16)) >>> 0;
  h = Math.imul(h, 0x7feb352d) >>> 0;
  h = (h ^ (h >>> 15)) >>> 0;
  h = Math.imul(h, 0x846ca68b) >>> 0;
  return (h ^ (h >>> 16)) >>> 0;
}

const utf8 = new TextEncoder();

// FNV-1a of a name's UTF-8 bytes: the per-kind salt.
export function fnv1a(name) {
  let h = 0x811c9dc5;
  for (const byte of utf8.encode(name)) h = Math.imul(h ^ byte, 0x01000193) >>> 0;
  return h >>> 0;
}

// The top 24 bits as 0..1, exact like the client's f32.
const unit = hash => (hash >>> 8) / 16777215;
const signedUnit = hash => unit(hash) * 2 - 1;

// ObjectStyle::jitter and ::scale_range.
export function styleJitter(style) {
  const jitter = Number(style?.jitter ?? 0);
  return Number.isFinite(jitter) ? Math.min(0.5, Math.max(0, jitter)) : 0;
}

export function styleScale(style) {
  const clamp = value => (Number.isFinite(value) ? Math.min(10, Math.max(0.1, value)) : 1);
  const [a, b] = (style?.scale ?? [1, 1]).map(Number).map(clamp);
  return [Math.min(a, b), Math.max(a, b)];
}

// SpriteVariant::pivot: the foot point in pixels inside the rect, y down.
export const variantPivot = variant => variant.pivot ?? [variant.rect[2] / 2, variant.rect[3]];

// place(): variant, mirroring, world size (tiles) and foot (tiles, +Y up) of one object.
// `tilesPerPx` is the size of one art pixel in tiles (1 / pixels_per_tile).
// snap_to_grid: moves the foot so the art's lower-left corner lies on the terrain pixel grid
// (`grid` tiles per texel, origin at 0; 0 leaves it).
function snapToGrid(foot, art, flip, pixel, grid) {
  if (!(grid > 0)) return foot;
  const [pivotX, pivotY] = variantPivot(art);
  const fromLeft = flip ? art.rect[2] - pivotX : pivotX;
  const fromBottom = art.rect[3] - pivotY;
  const corner = { x: foot.x - fromLeft * pixel, y: foot.y - fromBottom * pixel };
  return {
    x: foot.x + Math.round(corner.x / grid) * grid - corner.x,
    y: foot.y + Math.round(corner.y / grid) * grid - corner.y,
  };
}

export function placeObject(anchor, footprint, style, variants, salt, tilesPerPx, grid = 0) {
  const hash = channel => channelHash(anchor, salt, channel);
  const variant = hash(CHANNEL.variant) % Math.max(1, variants.length);
  const art = variants[Math.min(variant, variants.length - 1)];
  const flip = !!style?.flip && (hash(CHANNEL.flip) & 1) === 1;
  const [minScale, maxScale] = styleScale(style);
  const scale = minScale + (maxScale - minScale) * unit(hash(CHANNEL.scale));
  const jitter = styleJitter(style);
  const centerX = anchor.x + footprint.width / 2;
  const jittered = {
    x: centerX + jitter * signedUnit(hash(CHANNEL.jitterX)),
    y: anchor.y + jitter * (1 + signedUnit(hash(CHANNEL.jitterY))),
  };
  const size = { width: art.rect[2] * tilesPerPx * scale, height: art.rect[3] * tilesPerPx * scale };
  const foot = snapToGrid(jittered, art, flip, tilesPerPx * scale, grid);
  return { variant, art, flip, size, foot };
}
