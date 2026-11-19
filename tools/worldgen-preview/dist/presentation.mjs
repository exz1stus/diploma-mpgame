// Editable copy of the client's presentation files (crates/client/assets/world/{tiles,objects}.ron)
// plus the texture images the client would load from assets/textures/{tiles,objects}/<name>.png.
// Name resolution mirrors crates/client/src/world_presentation/{config,catalog}.rs.
import { DEFAULT_BLEND, sanitizeBlend } from './terrain.mjs';

export const TILE_TEXTURE_PX = 32;

// Sprite-sheet regions offered as texture sources (crates/worldgen/examples/textures.png).
// Rects are [x, y, width, height].
export const TILE_SOURCES = {
  regolith_highland: [288, 384, 32, 32],
  regolith_cracked: [320, 384, 32, 32],
  regolith_rocky: [352, 384, 32, 32],
  regolith_lowland: [384, 384, 32, 32],
  basalt: [384, 256, 32, 32],
  ice: [416, 128, 32, 32],
  snow: [384, 128, 32, 32],
};

const sprites = (prefix, rects) => Object.fromEntries(rects.map((rect, i) => [`${prefix}_${i}`, rect]));
// Pebble sprites are the densest 16 px quadrant of each pebble cell, one tile at 16 px per tile.
const PEBBLE_QUADRANTS = [[16, 0], [16, 0], [16, 0], [16, 0], [16, 16], [0, 0], [0, 0], [0, 0]];
export const OBJECT_SOURCES = {
  ...sprites('pebbles', PEBBLE_QUADRANTS.map(([dx, dy], i) => [i * 32 + dx, 384 + dy, 16, 16])),
  ...sprites('rock', [[8, 424, 15, 16], [40, 428, 16, 10], [65, 429, 21, 14], [103, 429, 18, 15], [133, 425, 23, 18], [165, 425, 23, 17], [196, 420, 19, 20], [228, 425, 23, 18], [165, 455, 18, 17], [197, 449, 24, 27], [228, 451, 19, 25]]),
  ...sprites('spikes', [[2, 447, 27, 32], [36, 447, 22, 32], [65, 447, 28, 32], [100, 447, 23, 32], [130, 448, 22, 31]]),
  ...sprites('boulder', [[3, 486, 26, 24], [32, 482, 28, 27], [66, 484, 25, 23], [101, 483, 22, 27], [135, 486, 16, 19], [168, 482, 21, 26], [195, 486, 22, 19], [230, 486, 21, 21]]),
  ...sprites('stalagmite_small', [[14, 277, 6, 9], [24, 270, 6, 16], [36, 266, 7, 20], [49, 264, 9, 23], [67, 262, 6, 25], [82, 261, 9, 26]]),
  ...sprites('stalagmite_large', [[134, 259, 16, 27], [164, 256, 21, 32], [197, 241, 19, 44], [258, 234, 26, 53]]),
  ...sprites('stalagmite_tall', [[101, 242, 24, 45], [197, 241, 19, 44], [228, 241, 21, 45]]),
};

// How a sheet region is colored when baked: `tint` multiplies its luminance by the
// texture's placeholder color, `blend` shifts its own colors toward it by `strength`.
export const TINTS = ['none', 'tint', 'blend'];

// ---------------------------------------------------------------- RON subset

// Parses the RON used by the presentation files: structs/tuples `( k: v )`, maps `{ "k": v }`,
// lists, strings, numbers, booleans, `Some(x)`/`None`, and comments.
export function parseRon(text) {
  let i = 0;
  const fail = message => { throw new Error(`RON: ${message} at ${i}`); };
  const skip = () => {
    for (;;) {
      while (i < text.length && /\s/.test(text[i])) i++;
      if (text.startsWith('//', i)) { while (i < text.length && text[i] !== '\n') i++; continue; }
      if (text.startsWith('/*', i)) { const end = text.indexOf('*/', i + 2); if (end < 0) fail('unclosed comment'); i = end + 2; continue; }
      return;
    }
  };
  const expect = char => { skip(); if (text[i] !== char) fail(`expected ${char}`); i++; };
  const string = () => {
    expect('"');
    let out = '';
    while (text[i] !== '"') {
      if (i >= text.length) fail('unclosed string');
      if (text[i] === '\\') { i++; out += { n: '\n', t: '\t', '"': '"', '\\': '\\' }[text[i]] ?? text[i]; } else out += text[i];
      i++;
    }
    i++;
    return out;
  };
  const value = () => {
    skip();
    const c = text[i];
    if (c === '"') return string();
    if (c === '[') {
      i++; const list = [];
      for (;;) { skip(); if (text[i] === ']') { i++; return list; } list.push(value()); skip(); if (text[i] === ',') i++; }
    }
    if (c === '{') {
      i++; const map = {};
      for (;;) {
        skip(); if (text[i] === '}') { i++; return map; }
        const key = value(); expect(':'); map[key] = value(); skip(); if (text[i] === ',') i++;
      }
    }
    if (c === '(') {
      i++; const struct = {};
      for (;;) {
        skip(); if (text[i] === ')') { i++; return struct; }
        const name = /^[A-Za-z_]\w*/.exec(text.slice(i))?.[0] ?? fail('expected field name');
        i += name.length; expect(':'); struct[name] = value(); skip(); if (text[i] === ',') i++;
      }
    }
    const number = /^[-+]?(\d+\.?\d*([eE][-+]?\d+)?|\.\d+)/.exec(text.slice(i));
    if (number) { i += number[0].length; return Number(number[0]); }
    const word = /^[A-Za-z_]\w*/.exec(text.slice(i))?.[0];
    if (word === 'true' || word === 'false') { i += word.length; return word === 'true'; }
    if (word === 'None') { i += 4; return null; }
    if (word === 'Some') { i += 4; expect('('); const inner = value(); expect(')'); return inner; }
    return fail('unexpected token');
  };
  const result = value();
  skip();
  if (i < text.length) fail('trailing input');
  return result;
}

const quote = value => JSON.stringify(String(value));
const sortedEntries = map => Object.entries(map || {}).sort(([a], [b]) => a.localeCompare(b));
const number = value => (Number.isInteger(value) ? `${value}.0` : String(value));

function ronMap(map, format = quote) {
  const entries = sortedEntries(map);
  return entries.length ? `{\n${entries.map(([key, value]) => `        ${quote(key)}: ${format(value)},`).join('\n')}\n    }` : '{}';
}

export function tilesRon(tiles) {
  const blend = sanitizeBlend(tiles.blend);
  return `// Ground textures and tints per biome. Exported by the worldgen preview (tools/worldgen-preview).
//
// A biome is drawn with the texture \`textures/tiles/<texture>.png\`. By default the texture
// name is the biome name; list a biome under \`textures\` to rename it or share one texture
// between several biomes. All tile textures must have the same size.
//
// Until a texture file exists, the tile is drawn flat with its placeholder color
// (keyed by texture name).
//
// A biome with no entry here and no texture file has no tiles of its own: it is drawn
// with the tiles of the terrain biome underneath (the biome the terrain map gives the
// tile). Feature biomes rely on this to override only objects.
//
// layers: texture name -> overlay layer (unlisted: 0), for height. Where textures on different
// layers meet, the higher one overlays the lower one with a crisp, noisy edge and shades the
// lower ground next to it; textures on the same layer blend. Give height bands increasing
// layers.
//
// tints: biome name -> color the ground is multiplied by (unlisted: white), for temperature.
// Tints are averaged over neighboring tiles, so they change in smooth gradients. Keep
// textures neutral and let tints color them.
//
// blend: pixels_per_tile (texture pixels per tile; textures repeat in world space),
// smooth_width (same-layer blend width in tiles, 0-1), overlay_reach (how far a higher layer
// reaches over a lower neighbor, 0-0.5 tiles), edge_noise (overlay edge irregularity, 0-1),
// noise_scale (its size in tiles), ao_strength (shade at the foot of higher layers, 0-1),
// ao_width (its width in pixels), tint_radius (tint smoothing radius in tiles).
(
    textures: ${ronMap(tiles.textures)},
    default_placeholder: ${quote(tiles.default_placeholder)},
    placeholder_colors: ${ronMap(tiles.placeholder_colors)},
    layers: ${ronMap(tiles.layers, value => String(Math.max(0, Math.round(value))))},
    tints: ${ronMap(tiles.tints)},
    blend: (
        pixels_per_tile: ${blend.pixels_per_tile},
        smooth_width: ${number(blend.smooth_width)},
        overlay_reach: ${number(blend.overlay_reach)},
        edge_noise: ${number(blend.edge_noise)},
        noise_scale: ${number(blend.noise_scale)},
        ao_strength: ${number(blend.ao_strength)},
        ao_width: ${blend.ao_width},
        tint_radius: ${blend.tint_radius},
    ),
)
`;
}

export function objectsRon(objects) {
  return `// Object sprites per object kind. Exported by the worldgen preview (tools/worldgen-preview).
//
// A kind is drawn with \`textures/objects/<texture>.png\`, stretched over its footprint.
// By default the texture name is the kind name; list a kind under \`textures\` to rename it
// or share a texture. Until a texture file exists, the object is drawn as a rectangle in
// its placeholder color (keyed by texture name).
(
    textures: ${ronMap(objects.textures)},
    default_placeholder: ${quote(objects.default_placeholder)},
    placeholder_colors: ${ronMap(objects.placeholder_colors)},
)
`;
}

// ---------------------------------------------------------------- client name resolution

export const textureFor = (table, name) => (Object.hasOwn(table.textures || {}, name) ? table.textures[name] : name);
export const placeholderOf = (table, texture) => table.placeholder_colors?.[texture] ?? table.default_placeholder;
export const listsName = (table, name) => Object.hasOwn(table.textures || {}, name)
  || Object.hasOwn(table.placeholder_colors || {}, textureFor(table, name));

// TextureCatalog::build: distinct textures in first-use order and the texture of each name.
export function textureCatalog(names, table) {
  const textures = [];
  const textureOf = names.map(name => {
    const texture = textureFor(table, name);
    let index = textures.findIndex(entry => entry.name === texture);
    if (index < 0) { textures.push({ name: texture, placeholder: placeholderOf(table, texture) }); index = textures.length - 1; }
    return index;
  });
  return { textures, textureOf };
}

// TileCatalog::build: plus which biomes have their own tiles, each texture's overlay layer,
// and each biome's tint (RGB, white when unlisted).
export function tileCatalog(biomeNames, tiles, hasFile) {
  const { textures, textureOf } = textureCatalog(biomeNames, tiles);
  return {
    textures, textureOf,
    ownTiles: biomeNames.map(name => listsName(tiles, name) || hasFile(textureFor(tiles, name))),
    overlay: textures.map(texture => Math.max(0, Math.round(tiles.layers?.[texture.name] ?? 0))),
    tints: biomeNames.map(name => hexRgb(tiles.tints?.[name] ?? '#FFFFFF')),
  };
}

// The biome whose tiles and tint a tile shows: its own, or its ground's.
export function drawnBiome(catalog, biome, ground) {
  return catalog.ownTiles[biome] ? biome : ground;
}

// TileCatalog::layer: the texture a tile is drawn with.
export function drawnTexture(catalog, biome, ground) {
  return catalog.textureOf[drawnBiome(catalog, biome, ground)] ?? 0;
}

// ---------------------------------------------------------------- defaults

const rulesOf = maps => Object.values(maps).flat();

// Overlay layers from the biome maps: within one temperature column, biomes stacked by height
// get increasing layers, so higher ground overlays lower ground; biomes side by side
// (temperature neighbors) at the same height rank share a layer and fade into each other. Feature
// biomes (volcanoes, ...) go above all terrain.
// featureMaps: one rule list per feature biome map.
export function defaultLayers(terrainRules, featureMaps, tiles) {
  const rank = rules => new Map(rules.map(rule => [rule, rules.filter(other => other !== rule
    && other.temperature.min < rule.temperature.max && rule.temperature.min < other.temperature.max
    && other.height.min < rule.height.min).length]));
  const layers = {};
  const raise = (name, layer) => {
    const texture = textureFor(tiles, name);
    layers[texture] = Math.max(layers[texture] ?? 0, layer);
  };
  const terrain = rank(terrainRules);
  for (const [rule, value] of terrain) raise(rule.name, value + 1);
  const top = Math.max(0, ...Object.values(layers));
  for (const rules of featureMaps) for (const [rule, value] of rank(rules)) raise(rule.name, top + 1 + value);
  return layers;
}

// Default ground surface of a biome: frost on frozen ground, basalt on volcanic ground,
// regolith elsewhere.
function surfaceOf(name, rules) {
  if (rules.length && rules.every(rule => rule.temperature.max <= -12)) return 'frost';
  return /basalt|volcan/.test(name) ? 'basalt' : 'regolith';
}

export const NEUTRAL_PLACEHOLDER = '#D0D0D0';

// Height-banded presentation, Minecraft style: biomes share neutral textures per surface and
// height layer (\`regolith_2\`, \`frost_1\`, ...) and get their color from tints. Layers come
// from defaultLayers; tints default to the biomes' old placeholder colors. Biomes without an
// entry keep drawing their ground.
export function bandedPresentation(tiles, terrainRules, featureMaps, biomeNames) {
  const biomeLayers = defaultLayers(terrainRules, featureMaps, { textures: {} });
  const rules = [...terrainRules, ...featureMaps.flat()];
  const out = {
    textures: {}, default_placeholder: tiles.default_placeholder ?? '#FF00FF', placeholder_colors: {},
    layers: {}, tints: { ...(tiles.tints || {}) }, blend: sanitizeBlend({ ...DEFAULT_BLEND, ...tiles.blend }),
  };
  for (const name of biomeNames) {
    const legacy = tiles.placeholder_colors?.[textureFor(tiles, name)];
    if (!listsName(tiles, name) || !Object.hasOwn(biomeLayers, name)) continue;
    const layer = biomeLayers[name];
    const texture = `${surfaceOf(name, rules.filter(rule => rule.name === name))}_${layer}`;
    out.textures[name] = texture;
    out.placeholder_colors[texture] = NEUTRAL_PLACEHOLDER;
    out.layers[texture] = layer;
    if (legacy && !out.tints[name]) out.tints[name] = legacy;
  }
  return out;
}

// Sheet sources for banded texture names: lower layers get rockier regolith, frost turns from
// ice to snow with height, basalt for volcanic ground. Art is baked neutral; tints color it.
export function defaultTileSource(textureName) {
  const match = /^(regolith|frost|basalt)_(\d+)$/.exec(textureName);
  if (!match) return null;
  const layer = Number(match[2]);
  if (match[1] === 'frost') return { id: layer <= 2 ? 'ice' : 'snow', tint: 'none', strength: 1 };
  if (match[1] === 'basalt') return { id: 'basalt', tint: 'tint', strength: 1 };
  const id = ['regolith_lowland', 'regolith_rocky', 'regolith_cracked'][layer - 1] ?? 'regolith_highland';
  return { id, tint: 'tint', strength: 1 };
}

const DEFAULT_OBJECT_SOURCES = {
  rock: { id: 'rock_0', tint: 'tint', strength: 1 },
  large_rock: { id: 'boulder_1', tint: 'tint', strength: 1 },
  iron_ore: { id: 'spikes_3', tint: 'tint', strength: 1 },
  pebbles: { id: 'pebbles_0', tint: 'tint', strength: 1 },
};

export function defaultObjectSource(textureName) {
  return DEFAULT_OBJECT_SOURCES[textureName] ? { ...DEFAULT_OBJECT_SOURCES[textureName] } : null;
}

export const DEFAULT_OBJECT_COLORS = { pebbles: '#77726B' };

// ---------------------------------------------------------------- baking

export function hexRgb(color) {
  const value = String(color || '').replace('#', '');
  const full = value.length === 3 ? [...value].map(c => c + c).join('') : value;
  const rgb = [0, 2, 4].map(i => parseInt(full.slice(i, i + 2), 16));
  return rgb.every(Number.isFinite) && full.length === 6 ? rgb : [255, 0, 255];
}

function regionMean(sheet, [x0, y0, width, height]) {
  const mean = [0, 0, 0];
  let count = 0;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const offset = ((y0 + y) * sheet.width + x0 + x) * 4;
    if (sheet.data[offset + 3] < 255) continue;
    for (let c = 0; c < 3; c++) mean[c] += sheet.data[offset + c];
    count++;
  }
  return mean.map(value => Math.max(1, value / Math.max(1, count)));
}

function tinted(sheet, rect, source, color) {
  const [x0, y0, width, height] = rect;
  const rgb = hexRgb(color);
  const mean = regionMean(sheet, rect);
  const strength = Number.isFinite(source.strength) ? source.strength : 1;
  const out = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const from = ((y0 + y) * sheet.width + x0 + x) * 4;
    const to = (y * width + x) * 4;
    for (let c = 0; c < 3; c++) {
      const value = sheet.data[from + c];
      out[to + c] = source.tint === 'tint' ? rgb[c] * (sheet.data[from] / mean[0]) * strength + value * (1 - strength)
        : source.tint === 'blend' ? value * (1 + strength * (rgb[c] / mean[c] - 1)) : value;
    }
    out[to + 3] = sheet.data[from + 3];
  }
  return { width, height, data: out };
}

// A tile texture from a sheet region, colored by its placeholder color.
export function bakeTileTexture(sheet, source, color) {
  const rect = TILE_SOURCES[source.id];
  return rect ? tinted(sheet, rect, source, color) : null;
}

// An object texture sized to its footprint at `pixelsPerTile`: the sprite at native size,
// standing on the bottom edge and centered (scaled down if it does not fit). The client
// stretches the texture over the footprint, so proportions survive.
export function bakeObjectTexture(sheet, source, color, footprint, pixelsPerTile) {
  const rect = OBJECT_SOURCES[source.id];
  if (!rect) return null;
  const sprite = tinted(sheet, rect, source, color);
  const width = Math.max(1, footprint.width * pixelsPerTile);
  const height = Math.max(1, footprint.height * pixelsPerTile);
  const scale = Math.min(1, width / sprite.width, height / sprite.height);
  const drawWidth = Math.max(1, Math.round(sprite.width * scale));
  const drawHeight = Math.max(1, Math.round(sprite.height * scale));
  const left = Math.floor((width - drawWidth) / 2);
  const top = height - drawHeight;
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < drawHeight; y++) for (let x = 0; x < drawWidth; x++) {
    const from = (Math.min(sprite.height - 1, Math.floor(y / scale)) * sprite.width + Math.min(sprite.width - 1, Math.floor(x / scale))) * 4;
    data.set(sprite.data.subarray(from, from + 4), ((top + y) * width + left + x) * 4);
  }
  return { width, height, data };
}

export function solidTexture(color, width = 1, height = 1) {
  const rgb = hexRgb(color);
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < width * height; i++) data.set([...rgb, 255], i * 4);
  return { width, height, data };
}

export function averageColor(image) {
  const sum = [0, 0, 0];
  let count = 0;
  for (let i = 0; i < image.width * image.height; i++) {
    if (image.data[i * 4 + 3] === 0) continue;
    for (let c = 0; c < 3; c++) sum[c] += image.data[i * 4 + c];
    count++;
  }
  return count ? sum.map(value => Math.round(value / count)) : [0, 0, 0];
}

// textures::stack_layers: every layer takes the size of the first real texture; missing
// textures and textures of another size become solid placeholder layers.
export function stackLayers(entries, images) {
  const first = images.find(Boolean);
  const width = first?.width ?? 1;
  const height = first?.height ?? 1;
  const warnings = [];
  const layers = entries.map((entry, i) => {
    const image = images[i];
    if (image && image.width === width && image.height === height) return Uint8Array.from(image.data);
    if (image) warnings.push(`${entry.name} is ${image.width}×${image.height}, expected ${width}×${height}; using its placeholder`);
    return Uint8Array.from(solidTexture(entry.placeholder, width, height).data);
  });
  return { width, height, layers, warnings };
}

export { DEFAULT_BLEND };
