// Objects are drawn like the client draws them (world/presentation/objects.rs): the kind's atlas
// sprite for the surface under it, placed by placement.mjs (variant, jitter, flip, scale, pivot on
// the foot), ground decals first and the rest by foot position; kinds without art are a rectangle
// in their placeholder color at 90 % of the footprint.
import { placeObject, variantPivot } from './placement.mjs';
import { hexRgb, spriteFor, spriteVariants } from './presentation.mjs';

export const PLACEHOLDER_SCALE = 0.9;

const STYLES = {
  rock: { label: 'Rock' },
  large_rock: { label: 'Large rock' },
  iron_ore: { label: 'Iron ore' },
  tree: { label: 'Tree' },
  pebbles: { label: 'Pebbles' },
};

export function objectStyle(type) {
  return Object.hasOwn(STYLES, type?.name) ? STYLES[type.name]
    : { label: type?.name?.replaceAll('_', ' ') || 'Object' };
}

export function objectKey(object) {
  return `${object.id.rule_id}:${object.id.anchor.x}:${object.id.anchor.y}`;
}

export function objectsVisibleAt(metrics) {
  return metrics.lod === 0 && metrics.baseTilePx >= 4;
}

export function objectAt(objects, x, y) {
  return objects.find(object => x >= object.id.anchor.x && y >= object.id.anchor.y
    && x < object.id.anchor.x + object.footprint.width
    && y < object.id.anchor.y + object.footprint.height) || null;
}

// Chunk responses repeat crossing instances. Draw and inspect each global ID once.
export function collectObjects(chunks, bounds) {
  const unique = new Map();
  for (const chunk of chunks) for (const object of chunk.objects) {
    const { x, y } = object.id.anchor;
    if (x < bounds.maxX && y < bounds.maxY
      && x + object.footprint.width > bounds.minX && y + object.footprint.height > bounds.minY) {
      unique.set(objectKey(object), object);
    }
  }
  return [...unique.values()].sort((a, b) =>
    (a.id.anchor.y + a.footprint.height) - (b.id.anchor.y + b.footprint.height)
    || a.id.anchor.x - b.id.anchor.x || a.id.rule_id - b.id.rule_id);
}

export function readObjectChunk(chunk, x, y, size, types, world) {
  if (chunk.position?.x !== x || chunk.position?.y !== y || chunk.size !== size || !Array.isArray(chunk.objects)) {
    throw new Error('Invalid object chunk');
  }
  const intersects = object => {
    const anchor = object?.id?.anchor;
    const footprint = object?.footprint;
    return Number.isInteger(object?.id?.rule_id) && object.id.rule_id >= 0 && object.id.rule_id <= 0xffffffff
      && Number.isInteger(object.kind) && !!types[object.kind]
      && Number.isSafeInteger(anchor?.x) && Number.isSafeInteger(anchor?.y) && anchor.x >= 0 && anchor.y >= 0
      && Number.isSafeInteger(footprint?.width) && Number.isSafeInteger(footprint?.height)
      && footprint.width > 0 && footprint.height > 0
      && footprint.width === types[object.kind].footprint.width && footprint.height === types[object.kind].footprint.height
      && anchor.x + footprint.width <= world.width && anchor.y + footprint.height <= world.height
      && anchor.x < (x + 1) * size && anchor.x + footprint.width > x * size
      && anchor.y < (y + 1) * size && anchor.y + footprint.height > y * size;
  };
  if (!chunk.objects.every(intersects)) throw new Error('Invalid placed object');
  // Ground tiles are already loaded as binary samples; keep only the overlay data.
  return { objects: chunk.objects };
}

// How one object draws, in tiles (+Y up). look: { sheet, recolored, atlas, tilesPerPx, kinds: [{ name,
// style, salt, placeholder }] }; under: { surface, tint } of the tile under its anchor (tint: its
// object tint), or null. Tinted kinds draw from the recolored sheet with the tint, others from the
// sheet in their own colors.
// → { ground, sortY, tinted, tint, sprite: { rect, flip, left, bottom, width, height } } or a placeholder
// rectangle { color, left, bottom, width, height }.
export function objectVisual(object, look, under) {
  const kind = look?.kinds[object.kind];
  if (!kind) return null;
  const surface = under?.surface ?? null;
  const ground = kind.style.layer === 'Ground';
  const { anchor } = object.id;
  const variants = look.sheet && spriteVariants(look.atlas, spriteFor(kind.style, kind.name, surface));
  if (!variants) {
    const { width, height } = object.footprint;
    const insetX = width * (1 - PLACEHOLDER_SCALE) / 2;
    const insetY = height * (1 - PLACEHOLDER_SCALE) / 2;
    return { ground, sortY: anchor.y, color: kind.placeholder, left: anchor.x + insetX, bottom: anchor.y + insetY, width: width - 2 * insetX, height: height - 2 * insetY };
  }
  const placement = placeObject(anchor, object.footprint, kind.style, variants, kind.salt, look.tilesPerPx);
  const { art, flip, size, foot } = placement;
  const [pivotX, pivotY] = variantPivot(art);
  const tilesPerArtPx = size.width / art.rect[2];
  return {
    ground, sortY: foot.y, tinted: kind.style.tinted !== false,
    tint: objectTint(kind.style, kind.style.tinted !== false ? under?.tint : null),
    sprite: {
      rect: art.rect, flip, width: size.width, height: size.height,
      // A mirrored sprite mirrors its pivot too, like the client's anchor.
      left: foot.x - (flip ? art.rect[2] - pivotX : pivotX) * tilesPerArtPx,
      bottom: foot.y - (art.rect[3] - pivotY) * tilesPerArtPx,
    },
  };
}

// objects.rs object_color: the kind's tint times the biome's object tint (sRGB, per channel); null
// if white.
export function objectTint(style, biomeTint) {
  const own = style.tint ? hexRgb(style.tint) : [255, 255, 255];
  const biome = biomeTint ?? [255, 255, 255];
  const tint = own.map((value, c) => Math.round(value * biome[c] / 255));
  return tint.every(value => value === 255) ? null : tint;
}

// A sprite multiplied by a tint, cached per sheet, rect and tint (canvases are cheap to reuse).
const tintedSprites = new WeakMap();
function tintedSprite(sheet, rect, tint) {
  if (!tintedSprites.has(sheet)) tintedSprites.set(sheet, new Map());
  const cache = tintedSprites.get(sheet);
  const key = `${rect.join(',')}|${tint.join(',')}`;
  if (!cache.has(key)) {
    const [sx, sy, sw, sh] = rect;
    const canvas = document.createElement('canvas');
    canvas.width = sw; canvas.height = sh;
    const context = canvas.getContext('2d');
    context.drawImage(sheet, sx, sy, sw, sh, 0, 0, sw, sh);
    context.globalCompositeOperation = 'multiply';
    context.fillStyle = `rgb(${tint.join(',')})`;
    context.fillRect(0, 0, sw, sh);
    // Multiply fills transparent pixels too; keep only the sprite's own shape.
    context.globalCompositeOperation = 'destination-in';
    context.drawImage(sheet, sx, sy, sw, sh, 0, 0, sw, sh);
    cache.set(key, canvas);
  }
  return cache.get(key);
}

// Draw order: ground decals first, then standing objects from north to south.
export function sortVisuals(visuals) {
  return visuals.sort((a, b) => Number(b.ground) - Number(a.ground) || b.sortY - a.sortY);
}

// The context is mirrored so +Y points up (see app.mjs); sprites are flipped back upright.
function drawVisual(ctx, visual, look, originX, originY, pixels) {
  const sheet = visual.tinted ? look.recolored ?? look.sheet : look.sheet;
  const sprite = visual.sprite;
  if (!sprite) {
    ctx.fillStyle = visual.color || '#FF00FF';
    ctx.fillRect(originX + visual.left * pixels, originY + visual.bottom * pixels, visual.width * pixels, visual.height * pixels);
    return;
  }
  const width = sprite.width * pixels;
  const height = sprite.height * pixels;
  ctx.save();
  ctx.translate(originX + sprite.left * pixels, originY + (sprite.bottom + sprite.height) * pixels);
  ctx.scale(sprite.flip ? -1 : 1, -1);
  const x = sprite.flip ? -width : 0;
  if (visual.tint) {
    ctx.drawImage(tintedSprite(sheet, sprite.rect, visual.tint), x, 0, width, height);
  } else {
    const [sx, sy, sw, sh] = sprite.rect;
    ctx.drawImage(sheet, sx, sy, sw, sh, x, 0, width, height);
  }
  ctx.restore();
}

// underAt(x, y): { surface, tint } of the tile under an object's anchor, or null.
export function drawObjects(ctx, objects, originX, originY, pixels, look, underAt, hovered = null) {
  ctx.save();
  ctx.imageSmoothingEnabled = false;
  const visuals = sortVisuals(objects
    .map(object => objectVisual(object, look, underAt(object.id.anchor.x, object.id.anchor.y)))
    .filter(Boolean));
  for (const visual of visuals) drawVisual(ctx, visual, look, originX, originY, pixels);
  if (hovered) {
    ctx.fillStyle = '#f1db822b'; ctx.strokeStyle = '#ffe59d'; ctx.lineWidth = 1.5;
    const x = originX + hovered.id.anchor.x * pixels;
    const y = originY + hovered.id.anchor.y * pixels;
    const width = hovered.footprint.width * pixels;
    const height = hovered.footprint.height * pixels;
    ctx.fillRect(x, y, width, height); ctx.strokeRect(x + .75, y + .75, width - 1.5, height - 1.5);
  }
  ctx.restore();
}

// Legend icons: each kind's default art (no surface), standing on the icon's bottom edge.
export function renderObjectLegend(legend, types, look) {
  legend.replaceChildren(...types.map((type, kind) => {
    const row = document.createElement('span');
    const icon = document.createElement('canvas');
    icon.width = 48; icon.height = 48; icon.setAttribute('aria-hidden', 'true');
    const context = icon.getContext('2d');
    context.setTransform(1, 0, 0, -1, 0, 48);
    context.imageSmoothingEnabled = false;
    const visual = objectVisual({ kind, id: { anchor: { x: 0, y: 0 } }, footprint: type.footprint }, look, null);
    if (visual) {
      const box = visual.sprite ?? visual;
      const scale = 44 / Math.max(box.width, box.height);
      const offsetX = (48 - box.width * scale) / 2 - box.left * scale;
      const offsetY = 2 - box.bottom * scale;
      drawVisual(context, visual, look, offsetX, offsetY, scale);
    }
    const name = document.createElement('span');
    name.textContent = objectStyle(type).label;
    const dimensions = document.createElement('small');
    dimensions.textContent = `${type.footprint.width} × ${type.footprint.height}`;
    row.append(icon, name, dimensions);
    return row;
  }));
}
