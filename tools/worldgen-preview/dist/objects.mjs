// Objects are drawn like the client draws them (world_presentation/objects.rs): the kind's
// texture stretched over the footprint, or a rectangle in its placeholder color at 90 %.
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

// look(kind) → { image: CanvasImageSource | null, color: '#RRGGBB' }. The context is
// mirrored so +Y points up (see app.mjs); images are flipped back upright.
export function drawObject(ctx, look, x, y, width, height) {
  if (look?.image) {
    ctx.save();
    ctx.translate(x, y + height);
    ctx.scale(1, -1);
    ctx.drawImage(look.image, 0, 0, width, height);
    ctx.restore();
    return;
  }
  ctx.fillStyle = look?.color || '#FF00FF';
  const insetX = width * (1 - PLACEHOLDER_SCALE) / 2;
  const insetY = height * (1 - PLACEHOLDER_SCALE) / 2;
  ctx.fillRect(x + insetX, y + insetY, width - 2 * insetX, height - 2 * insetY);
}

export function drawObjects(ctx, objects, originX, originY, pixels, look, hovered = null) {
  ctx.save();
  ctx.imageSmoothingEnabled = false;
  for (const object of objects) {
    drawObject(ctx, look(object.kind), originX + object.id.anchor.x * pixels, originY + object.id.anchor.y * pixels,
      object.footprint.width * pixels, object.footprint.height * pixels);
  }
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

export function renderObjectLegend(legend, types, look) {
  legend.replaceChildren(...types.map((type, kind) => {
    const row = document.createElement('span');
    const icon = document.createElement('canvas');
    icon.width = 48; icon.height = 48; icon.setAttribute('aria-hidden', 'true');
    const context = icon.getContext('2d');
    // Icons keep the footprint's aspect ratio, as the client stretches textures over it.
    const scale = 44 / Math.max(type.footprint.width, type.footprint.height);
    const width = type.footprint.width * scale;
    const height = type.footprint.height * scale;
    context.setTransform(1, 0, 0, -1, 0, 48);
    context.imageSmoothingEnabled = false;
    drawObject(context, look(kind), (48 - width) / 2, (48 - height) / 2, width, height);
    const name = document.createElement('span');
    name.textContent = objectStyle(type).label;
    const dimensions = document.createElement('small');
    dimensions.textContent = `${type.footprint.width} × ${type.footprint.height}`;
    row.append(icon, name, dimensions);
    return row;
  }));
}
