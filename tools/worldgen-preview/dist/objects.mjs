const STYLES = {
  rock: { label: 'Rock', color: '#a6b2bd', shade: '#5b6d7e' },
  large_rock: { label: 'Large rock', color: '#c1ccd3', shade: '#657b8a' },
  iron_ore: { label: 'Iron ore', color: '#e7a15a', shade: '#78564e' },
  tree: { label: 'Tree', color: '#66bc82', shade: '#276247' },
};

export function objectStyle(type) {
  return Object.hasOwn(STYLES, type?.name) ? STYLES[type.name]
    : { label: type?.name?.replaceAll('_', ' ') || 'Object', color: '#c5a6e5', shade: '#685980' };
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

// Symbols use unit coordinates, so their occupied footprint follows the map zoom.
export function drawObjectSymbol(ctx, type, x, y, width, height) {
  const style = objectStyle(type);
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(width, height);
  ctx.lineJoin = 'round';
  ctx.lineWidth = 1.25 / Math.min(width, height);
  ctx.fillStyle = '#07151b65';
  ctx.beginPath(); ctx.ellipse(.53, .76, .38, .16, 0, 0, Math.PI * 2); ctx.fill();
  if (type?.name === 'tree') {
    ctx.fillStyle = '#684f3b';
    ctx.fillRect(.43, .57, .14, .3);
    ctx.strokeStyle = '#183e30';
    ctx.fillStyle = style.shade;
    ctx.beginPath(); ctx.arc(.5, .44, .34, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
    ctx.fillStyle = style.color;
    ctx.beginPath(); ctx.arc(.39, .34, .22, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = '#8fd49b';
    ctx.beginPath(); ctx.arc(.43, .24, .11, 0, Math.PI * 2); ctx.fill();
  } else {
    ctx.strokeStyle = '#263c49';
    ctx.fillStyle = style.shade;
    ctx.beginPath();
    ctx.moveTo(.12, .66); ctx.lineTo(.24, .27); ctx.lineTo(.57, .13);
    ctx.lineTo(.82, .35); ctx.lineTo(.9, .7); ctx.lineTo(.58, .86); ctx.lineTo(.25, .81);
    ctx.closePath(); ctx.fill(); ctx.stroke();
    ctx.fillStyle = type?.name === 'iron_ore' ? '#a88c7b' : style.color;
    ctx.beginPath(); ctx.moveTo(.24, .3); ctx.lineTo(.56, .17); ctx.lineTo(.79, .37);
    ctx.lineTo(.52, .57); ctx.lineTo(.17, .62); ctx.closePath(); ctx.fill();
    ctx.strokeStyle = '#e5eef04d';
    ctx.beginPath(); ctx.moveTo(.52, .57); ctx.lineTo(.57, .78); ctx.stroke();
    if (type?.name === 'iron_ore') {
      for (const [cx, cy, radius] of [[.35, .42, .11], [.65, .55, .13], [.4, .71, .08]]) {
        ctx.fillStyle = style.color;
        ctx.beginPath(); ctx.moveTo(cx, cy - radius); ctx.lineTo(cx + radius, cy);
        ctx.lineTo(cx, cy + radius); ctx.lineTo(cx - radius, cy); ctx.closePath(); ctx.fill();
      }
    }
  }
  ctx.restore();
}

export function drawObjects(ctx, objects, types, originX, originY, pixels, hovered = null) {
  for (const object of objects) {
    const x = originX + object.id.anchor.x * pixels;
    const y = originY + object.id.anchor.y * pixels;
    const width = object.footprint.width * pixels;
    const height = object.footprint.height * pixels;
    drawObjectSymbol(ctx, types[object.kind], x, y, width, height);
  }
  if (hovered) {
    ctx.save();
    ctx.fillStyle = '#f1db822b'; ctx.strokeStyle = '#ffe59d'; ctx.lineWidth = 1.5;
    const x = originX + hovered.id.anchor.x * pixels;
    const y = originY + hovered.id.anchor.y * pixels;
    const width = hovered.footprint.width * pixels;
    const height = hovered.footprint.height * pixels;
    ctx.fillRect(x, y, width, height); ctx.strokeRect(x + .75, y + .75, width - 1.5, height - 1.5);
    ctx.restore();
  }
}

export function renderObjectLegend(legend, types) {
  legend.replaceChildren(...types.map(type => {
    const row = document.createElement('span');
    const icon = document.createElement('canvas');
    icon.width = 48; icon.height = 48; icon.setAttribute('aria-hidden', 'true');
    drawObjectSymbol(icon.getContext('2d'), type, 0, 0, 48, 48);
    const name = document.createElement('span');
    name.textContent = objectStyle(type).label;
    const dimensions = document.createElement('small');
    dimensions.textContent = `${type.footprint.width} × ${type.footprint.height}`;
    row.append(icon, name, dimensions);
    return row;
  }));
}
