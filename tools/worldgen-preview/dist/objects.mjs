import { ATLAS_URL, SOURCE_TILE_PX, SPRITES, objectIconSprite, objectSprite, spriteHash } from './sprites.mjs';

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

let atlas = null;
let atlasRequest = null;
const setMeans = new Map();
const tinted = new Map();

// Loads the shared sprite sheet once; callers redraw when the promise settles.
export function loadObjectSprites() {
  atlasRequest ??= new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      context.drawImage(image, 0, 0);
      atlas = context.getImageData(0, 0, canvas.width, canvas.height);
      resolve(atlas);
    };
    image.onerror = () => { atlasRequest = null; reject(new Error('Texture atlas unavailable')); };
    image.src = ATLAS_URL;
  });
  return atlasRequest;
}

export function objectSpritesReady() { return atlas !== null; }

function setMean(set) {
  if (!setMeans.has(set)) {
    const mean = [0, 0, 0];
    let count = 0;
    for (const rect of SPRITES[set]) for (let y = 0; y < rect.height; y++) for (let x = 0; x < rect.width; x++) {
      const offset = ((rect.y + y) * atlas.width + rect.x + x) * 4;
      if (atlas.data[offset + 3] < 255) continue;
      for (let c = 0; c < 3; c++) mean[c] += atlas.data[offset + c];
      count++;
    }
    setMeans.set(set, mean.map(value => value / Math.max(1, count)));
  }
  return setMeans.get(set);
}

// Tinted sprites match the ground tint rules: gray art takes the full biome color, slate
// stalagmites keep part of their hue. Ore nuggets are baked onto opaque lower pixels.
function spriteCanvas(choice, tint) {
  const key = `${choice.set}:${choice.index}:${tint?.join(',') ?? 'none'}:${choice.ore}`;
  if (tinted.has(key)) return tinted.get(key);
  const { rect } = choice;
  const pixels = new ImageData(rect.width, rect.height);
  const mean = setMean(choice.set);
  for (let y = 0; y < rect.height; y++) for (let x = 0; x < rect.width; x++) {
    const source = ((rect.y + y) * atlas.width + rect.x + x) * 4;
    const out = (y * rect.width + x) * 4;
    for (let c = 0; c < 3; c++) {
      const value = atlas.data[source + c];
      pixels.data[out + c] = !tint ? value : choice.tint.tint === 'luminance' ? tint[c] * atlas.data[source] / mean[0]
        : value * (1 + choice.tint.strength * (tint[c] / mean[c] - 1));
    }
    pixels.data[out + 3] = atlas.data[source + 3];
  }
  if (choice.ore) {
    const spots = [];
    for (let y = Math.floor(rect.height * .45); y < rect.height - 3; y++) for (let x = 2; x < rect.width - 2; x++) {
      if ([[0, 0], [-1, 0], [1, 0], [0, -1], [0, 1]].every(([dx, dy]) => pixels.data[((y + dy) * rect.width + x + dx) * 4 + 3] === 255)) spots.push([x, y]);
    }
    for (let i = 0; i < Math.min(3, spots.length); i++) {
      const [x, y] = spots[spriteHash(choice.index, i, rect.x) % spots.length];
      for (const [dx, dy, color] of [[0, -1, [255, 196, 120]], [-1, 0, [231, 161, 90]], [1, 0, [176, 98, 52]], [0, 0, [231, 161, 90]], [0, 1, [120, 70, 46]]]) {
        pixels.data.set(color, ((y + dy) * rect.width + x + dx) * 4);
      }
    }
  }
  const canvas = document.createElement('canvas');
  canvas.width = rect.width; canvas.height = rect.height;
  canvas.getContext('2d').putImageData(pixels, 0, 0);
  if (tinted.size > 4096) tinted.clear();
  tinted.set(key, canvas);
  return canvas;
}

// Sprites keep their native 16 source pixels per tile, stand on the footprint's bottom edge,
// and are centered horizontally. Types without art fall back to vector symbols.
function drawObjectSprite(ctx, choice, tint, x, y, width, height, pixels) {
  const image = spriteCanvas(choice, tint);
  const scale = pixels / SOURCE_TILE_PX;
  const drawWidth = choice.rect.width * scale;
  const drawHeight = choice.rect.height * scale;
  const left = x + (width - drawWidth) / 2;
  const bottom = y + height - scale;
  ctx.save();
  ctx.imageSmoothingEnabled = scale < 1;
  ctx.drawImage(image, left, bottom - drawHeight, drawWidth, drawHeight);
  ctx.restore();
}

export function drawObjects(ctx, objects, types, originX, originY, pixels, hovered = null, environment = () => null) {
  for (const object of objects) {
    const x = originX + object.id.anchor.x * pixels;
    const y = originY + object.id.anchor.y * pixels;
    const width = object.footprint.width * pixels;
    const height = object.footprint.height * pixels;
    const context = environment(object);
    const choice = atlas ? objectSprite(types[object.kind]?.name, object, context?.surface) : null;
    if (choice) drawObjectSprite(ctx, choice, context?.tint ?? null, x, y, width, height, pixels);
    else drawObjectSymbol(ctx, types[object.kind], x, y, width, height);
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
    const choice = atlas ? objectIconSprite(type.name) : null;
    const context = icon.getContext('2d');
    if (choice) {
      // Largest crisp integer scale that fits the icon, centered.
      const scale = Math.max(1, Math.floor(44 / Math.max(choice.rect.width, choice.rect.height)));
      const width = choice.rect.width * scale;
      const height = choice.rect.height * scale;
      context.imageSmoothingEnabled = false;
      context.drawImage(spriteCanvas(choice, null), (48 - width) / 2, (48 - height) / 2, width, height);
    } else drawObjectSymbol(context, type, 0, 0, 48, 48);
    const name = document.createElement('span');
    name.textContent = objectStyle(type).label;
    const dimensions = document.createElement('small');
    dimensions.textContent = `${type.footprint.width} × ${type.footprint.height}`;
    row.append(icon, name, dimensions);
    return row;
  }));
}
