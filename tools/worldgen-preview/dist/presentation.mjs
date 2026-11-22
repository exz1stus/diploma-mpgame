// Editable copy of the client's presentation files (crates/client/assets/world/{tiles,objects}.ron);
// art comes from the client's atlas (world/atlas.ron and the sheet it names), never edited here.
// Name resolution mirrors crates/client/src/world/presentation/{config,catalog}.rs.
import { DEFAULT_BLEND, sanitizeBlend, srgbToLinear } from './terrain.mjs';
import { styleJitter, styleScale } from './placement.mjs';

// ---------------------------------------------------------------- RON subset

// Parses the RON used by the presentation files: structs `( k: v )`, tuples `(a, b)` (as lists),
// maps `{ "k": v }`, lists, strings, numbers, booleans, `Some(x)`/`None`, unit enum variants (as
// their name), and comments.
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
      i++; skip();
      // Positional tuples, like atlas rects `(x, y, w, h)`, become lists.
      if (text[i] !== ')' && !/^[A-Za-z_]\w*\s*:/.test(text.slice(i))) {
        const tuple = [];
        for (;;) { skip(); if (text[i] === ')') { i++; return tuple; } tuple.push(value()); skip(); if (text[i] === ',') i++; }
      }
      const struct = {};
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
    // Unit enum variants, like `layer: Ground`, read as their name.
    if (word) { i += word.length; return word; }
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
// A biome is drawn with a texture. By default the texture name is the biome name; list a
// biome under \`textures\` to rename it or share one texture between several biomes.
//
// regions: texture name -> atlas tile region (atlas.ron \`tiles\`) its art comes from; unlisted
// textures use the region of their own name. Textures keep their own layer and placeholder
// even when they share a region. Until a texture's region exists, the tile is drawn flat
// with its placeholder color (keyed by texture name).
//
// recolor: true makes tile art neutral: each pixel becomes its texture's placeholder color
// scaled by the pixel's brightness relative to the tile's average, so the art keeps its light
// and shade but not its hue, and tints alone color the ground. Use false for art that is
// already neutral (grayscale) or should keep its own colors.
//
// A biome with no entry here and no atlas region has no tiles of its own: it is drawn
// with the tiles of the terrain biome underneath (the biome the terrain map gives the
// tile). Feature biomes rely on this to override only objects.
//
// layers: texture name -> overlay layer (unlisted: 0), for height. Where textures on different
// layers meet, the higher one overlays the lower one with a crisp, noisy edge and shades the
// lower ground next to it. Give height bands increasing layers. Within a layer, biomes meet
// on crisp, noisy edges too, without shading.
//
// tints: biome name -> color the ground is multiplied by (unlisted: white), for temperature.
// Keep textures neutral and let tints color them; close tints make biomes read as terraces.
//
// blend: pixels_per_tile (texture pixels per tile; textures repeat in world space),
// overlay_reach (how far a higher layer reaches over a lower neighbor, 0-0.5 tiles),
// edge_noise (irregularity of height and biome edges, 0-1), noise_scale (its size in tiles),
// ao_strength (shade at the foot of higher layers, 0-1),
// ao_width (its width in pixels), ao_south / ao_north (shade scale where the higher ground
// lies north / south of the lower ground, 0-4), tint_radius (radius in tiles over which tints
// are averaged within a layer; 0 keeps each biome one flat color), height_shade (brightness
// per unit of terrain height away from height_reference, 0-10: lower ground darker, higher
// brighter, so craters read as dark bowls), height_reference (the height drawn unchanged,
// 0-1), slope_shade (light on slopes facing the north-west and shade on the others, 0-5);
// the height and slope shading vary the light smoothly over the land, so repeated textures
// do not read as tiles.
(
    textures: ${ronMap(tiles.textures)},
    default_placeholder: ${quote(tiles.default_placeholder)},
    placeholder_colors: ${ronMap(tiles.placeholder_colors)},
    regions: ${ronMap(tiles.regions)},
    recolor: ${tiles.recolor === true},
    layers: ${ronMap(tiles.layers, value => String(Math.max(0, Math.round(value))))},
    tints: ${ronMap(tiles.tints)},
    blend: (
        pixels_per_tile: ${blend.pixels_per_tile},
        overlay_reach: ${number(blend.overlay_reach)},
        edge_noise: ${number(blend.edge_noise)},
        noise_scale: ${number(blend.noise_scale)},
        ao_strength: ${number(blend.ao_strength)},
        ao_width: ${blend.ao_width},
        ao_south: ${number(blend.ao_south)},
        ao_north: ${number(blend.ao_north)},
        tint_radius: ${blend.tint_radius},
        height_shade: ${number(blend.height_shade)},
        height_reference: ${number(blend.height_reference)},
        slope_shade: ${number(blend.slope_shade)},
    ),
)
`;
}

// One kind's style, writing only the fields that differ from the client's defaults.
function styleRon(style) {
  const fields = [];
  if (style.sprite) fields.push(`sprite: Some(${quote(style.sprite)})`);
  if (Object.keys(style.sprites || {}).length) {
    const entries = sortedEntries(style.sprites).map(([surface, sprite]) => `                ${quote(surface)}: ${quote(sprite)},`);
    fields.push(`sprites: {\n${entries.join('\n')}\n            }`);
  }
  const jitter = styleJitter(style);
  if (jitter) fields.push(`jitter: ${number(jitter)}`);
  if (style.flip) fields.push('flip: true');
  const [min, max] = styleScale(style);
  if (min !== 1 || max !== 1) fields.push(`scale: (${number(min)}, ${number(max)})`);
  if (style.layer === 'Ground') fields.push('layer: Ground');
  if (style.tinted === false) fields.push('tinted: false');
  if (style.shadow === false) fields.push('shadow: false');
  if (style.occlusion === false) fields.push('occlusion: false');
  if (style.tint) fields.push(`tint: Some(${quote(style.tint)})`);
  if (style.placeholder) fields.push(`placeholder: Some(${quote(style.placeholder)})`);
  return fields.length ? `(\n${fields.map(field => `            ${field},`).join('\n')}\n        )` : '()';
}

// A `name: (key: value, ...)` style line, when the file sets one (the client defaults it).
// Keys in `integers` are whole numbers (u32 on the client); the rest are floats.
function numbersRon(name, values, keys, integers = []) {
  if (!values) return '';
  const value = key => (integers.includes(key) ? String(Math.max(0, Math.round(values[key]))) : number(values[key]));
  const fields = keys.filter(key => Number.isFinite(values[key])).map(key => `${key}: ${value(key)}`);
  return `\n    ${name}: (${fields.join(', ')}),`;
}

export function objectsRon(objects) {
  const kinds = sortedEntries(objects.kinds);
  return `// How object kinds are drawn. Exported by the worldgen preview (tools/worldgen-preview).
// Art comes from atlas.ron \`sprites\`; one variant per object is picked from its anchor tile,
// so every client draws the same world.
//
// recolor: Some("#RRGGBB") makes the art of tinted kinds neutral in this color first (each sprite
//   keeps its light and shade, relative to its own average), so the tint alone colors it. None
//   keeps the art's own colors (use for art that is already neutral).
// tints: biome name -> color the art of tinted kinds standing there is multiplied by; kept apart
//   from the ground tints so objects stand out. Feature biomes without an entry use their ground
//   biome's; other unlisted biomes stay white.
//
// shadow: sun shadows of standing objects (the sun follows the world clock, clock.ron):
//   opacity (darkness under a high sun, 0-1), max_length (longest shadow, in object heights,
//   for a low sun), fade_degrees (sun elevation over which shadows fade out toward dusk), depth
//   (how deep objects are, as a fraction of their height: the least a shadow spreads, so side
//   light does not flatten it to a line). Shadows darken whole terrain texels.
// occlusion: ambient occlusion in whole terrain texels, like the tiles' height shading:
//   opacity (ground darkness next to an object, 0-1), reach (texels it fades over around
//   standing art, 1-6), decal_reach (around ground decals like pebbles, 0-6), base_rows (bottom
//   rows of standing art that darken so it sits in the ground, 0-8), base_opacity (0-1).
//
// surfaces: biome name -> surface. A kind's \`sprites\` choose art per surface, so a rock on
//   basalt looks like basalt. Feature biomes without an entry use their ground biome's.
//
// kinds: object kind -> style; every field is optional.
//   sprite:      atlas sprite where \`sprites\` lists no art for the surface (default: kind name)
//   sprites:     surface -> atlas sprite
//   jitter:      largest nudge of the foot off its footprint's south-edge center, in tiles
//                (0 to 0.5); it only moves north, east and west, staying on the footprint
//   flip:        mirror half of the objects; only for art without painted light (default: off)
//   scale:       (min, max) size multiplier, picked per object; anything but 1 puts the art's
//                pixels off the terrain pixel grid the art otherwise snaps to
//   layer:       Sorted (stands; drawn by foot position with players) or Ground (flat decal)
//   tinted:      false keeps the art's own colors: no recolor and no biome tint (default: true)
//   shadow:      false casts no sun shadow (default: true; ground decals never cast one)
//   occlusion:   false skips the ambient occlusion at the lower edge (default: true)
//   tint:        Some("#RRGGBB") multiplies the art
//   placeholder: Some("#RRGGBB") rectangle drawn while the kind has no sprite
// Art is drawn at its pixel size, \`tiles.ron\` blend.pixels_per_tile pixels per tile.
(
    default_placeholder: ${quote(objects.default_placeholder)},
    recolor: ${objects.recolor ? `Some(${quote(objects.recolor)})` : 'None'},
    tints: ${ronMap(objects.tints)},${numbersRon('shadow', objects.shadow, ['opacity', 'max_length', 'fade_degrees', 'depth'])}${numbersRon('occlusion', objects.occlusion, ['opacity', 'reach', 'decal_reach', 'base_rows', 'base_opacity'], ['reach', 'decal_reach', 'base_rows'])}
    surfaces: ${ronMap(objects.surfaces)},
    kinds: ${kinds.length ? `{\n${kinds.map(([kind, style]) => `        ${quote(kind)}: ${styleRon(style)},`).join('\n')}\n    }` : '{}'},
)
`;
}

// ---------------------------------------------------------------- atlas

// The client's atlas.ron: { image, tiles: { name: [x, y, w, h] }, sprites: { name: [{ rect, pivot }] } }.
export function readAtlas(text) {
  const atlas = parseRon(text);
  return { image: atlas.image, tiles: atlas.tiles || {}, sprites: atlas.sprites || {} };
}

// TileTable::region_for and PresentationConfig::tile_region.
export const regionFor = (tiles, texture) => (Object.hasOwn(tiles.regions || {}, texture) ? tiles.regions[texture] : texture);
export const tileRegion = (atlas, tiles, texture) => atlas?.tiles[regionFor(tiles, texture)] ?? null;

// Atlas::sprite: a sprite's variants, or null when it is missing or empty.
export const spriteVariants = (atlas, name) => (atlas?.sprites[name]?.length ? atlas.sprites[name] : null);

// SheetPixels::crop: the pixels of a rect as their own image; null outside the sheet.
export function cropRegion(sheet, [x0, y0, width, height]) {
  if (!sheet || width <= 0 || height <= 0 || x0 + width > sheet.width || y0 + height > sheet.height) return null;
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    const from = ((y0 + y) * sheet.width + x0) * 4;
    data.set(sheet.data.subarray(from, from + width * 4), y * width * 4);
  }
  return { width, height, data };
}

// textures::recolor: neutral art in `color`, scaled by each pixel's red over the tile's mean red
// (opaque pixels), so tints alone color the ground.
export function recolorImage(image, color) {
  const rgb = hexRgb(color);
  let sum = 0;
  let count = 0;
  for (let i = 0; i < image.data.length; i += 4) if (image.data[i + 3] === 255) { sum += image.data[i]; count++; }
  const mean = Math.max(1, sum / Math.max(1, count));
  const data = Uint8ClampedArray.from(image.data);
  for (let i = 0; i < data.length; i += 4) {
    const light = image.data[i] / mean;
    for (let c = 0; c < 3; c++) data[i + c] = Math.min(255, Math.max(0, Math.round(rgb[c] * light)));
  }
  return { width: image.width, height: image.height, data };
}

// textures::recolor_regions: a copy of the sheet with each region recolored on its own mean.
export function recolorRegions(sheet, rects, color) {
  const out = { width: sheet.width, height: sheet.height, data: Uint8ClampedArray.from(sheet.data) };
  for (const rect of rects) {
    const crop = cropRegion(sheet, rect);
    if (!crop) continue;
    const neutral = recolorImage(crop, color);
    const [x0, y0, width] = rect;
    for (let y = 0; y < crop.height; y++) {
      out.data.set(neutral.data.subarray(y * width * 4, (y + 1) * width * 4), ((y0 + y) * sheet.width + x0) * 4);
    }
  }
  return out;
}

// Atlas::sprite_rects: every sprite variant's rect, once.
export function spriteRects(atlas) {
  const seen = new Map();
  for (const variants of Object.values(atlas?.sprites ?? {})) for (const { rect } of variants) seen.set(rect.join(','), rect);
  return [...seen.values()];
}

// ObjectStyles::style and ObjectStyle::sprite_for.
export const objectStyleOf = (objects, kind) => objects.kinds?.[kind] ?? {};
export const spriteFor = (style, kind, surface) => (surface && style.sprites?.[surface]) || style.sprite || kind;
export const objectPlaceholder = (objects, style) => style.placeholder || objects.default_placeholder;

// ObjectCatalog::surface: a tile's biome's surface, else its ground biome's.
export function objectSurface(objects, biomeNames, biome, ground) {
  const of = id => objects.surfaces?.[biomeNames[id]] ?? null;
  return of(biome) ?? of(ground);
}

// ObjectCatalog::tint: a tile's biome's object tint, else its ground biome's; null is white.
export function objectBiomeTint(objects, biomeNames, biome, ground) {
  const of = id => objects.tints?.[biomeNames[id]] ?? null;
  const hex = of(biome) ?? of(ground);
  return hex ? hexRgb(hex) : null;
}

// Files written before per-kind styles (textures / placeholder_colors) keep their colors.
export function migrateObjects(objects) {
  if (objects.kinds) {
    return {
      default_placeholder: objects.default_placeholder ?? '#475A47', tints: objects.tints || {}, shadow: objects.shadow ?? null,
      occlusion: objects.occlusion ?? null,
      recolor: objects.recolor ?? null, surfaces: objects.surfaces || {}, kinds: objects.kinds,
    };
  }
  const kinds = {};
  for (const [texture, color] of Object.entries(objects.placeholder_colors || {})) kinds[texture] = { placeholder: color };
  for (const [kind, texture] of Object.entries(objects.textures || {})) kinds[kind] = { ...kinds[texture], sprite: texture };
  return { default_placeholder: objects.default_placeholder ?? '#475A47', tints: {}, recolor: null, surfaces: {}, kinds };
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

// Temperature palette for tints: close, muted colors from snow white through frost blue-gray,
// neutral gray and warm gray to tan, ochre, copper and rust. [°C, sRGB].
export const TEMPERATURE_RAMP = [
  [-50, '#E8EEF4'], [-30, '#D3DDE6'], [-18, '#BEC8D0'], [-8, '#B0B6BA'], [0, '#ADADAA'],
  [6, '#B3AB9E'], [12, '#BAA78E'], [18, '#C0A07E'], [26, '#BE9271'], [35, '#B07E63'], [45, '#9E6C56'],
];

const linearToSrgbByte = value => Math.round(Math.min(1, Math.max(0, value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055)) * 255);

// The ramp color at a temperature (interpolated in linear light), brightened slightly with
// height so higher bands read a little lighter: ×0.94 at height 0 to ×1.06 at height 1.
export function rampTint(temperature, height = 0.5) {
  const ramp = TEMPERATURE_RAMP;
  let i = 0;
  while (i < ramp.length - 2 && temperature > ramp[i + 1][0]) i++;
  const [t0, c0] = ramp[i];
  const [t1, c1] = ramp[i + 1];
  const k = Math.min(1, Math.max(0, (temperature - t0) / (t1 - t0)));
  const light = 0.94 + 0.12 * Math.min(1, Math.max(0, height));
  const a = hexRgb(c0);
  const b = hexRgb(c1);
  const rgb = a.map((value, c) => {
    const mixed = srgbToLinear(value / 255) * (1 - k) + srgbToLinear(b[c] / 255) * k;
    return linearToSrgbByte(mixed * light);
  });
  return `#${rgb.map(value => value.toString(16).padStart(2, '0')).join('').toUpperCase()}`;
}

// Ramp tints for every biome with rules: the color at the midpoint of its temperature and
// height ranges (averaged over its rules). Unbounded ranges are clamped to the ramp.
export function rampTints(rules, biomeNames) {
  const tints = {};
  const [low, high] = [TEMPERATURE_RAMP[0][0], TEMPERATURE_RAMP.at(-1)[0]];
  const clamp = value => Math.min(high, Math.max(low, value));
  for (const name of biomeNames) {
    const own = rules.filter(rule => rule.name === name);
    if (!own.length) continue;
    const temperature = own.reduce((sum, rule) => sum + (clamp(rule.temperature.min) + clamp(rule.temperature.max)) / 2, 0) / own.length;
    const height = own.reduce((sum, rule) => sum + (rule.height.min + rule.height.max) / 2, 0) / own.length;
    tints[name] = rampTint(temperature, height);
  }
  return tints;
}

// Height-banded presentation, Minecraft style: biomes share neutral textures per surface and
// height layer (\`regolith_2\`, \`frost_1\`, ...) and get their color from tints. Layers come
// from defaultLayers; tints from the temperature ramp. Biomes without an entry keep drawing
// their ground.
export function bandedPresentation(tiles, terrainRules, featureMaps, biomeNames) {
  const biomeLayers = defaultLayers(terrainRules, featureMaps, { textures: {} });
  const rules = [...terrainRules, ...featureMaps.flat()];
  const out = {
    textures: {}, default_placeholder: tiles.default_placeholder ?? '#FF00FF', placeholder_colors: {},
    layers: {}, tints: {}, blend: sanitizeBlend({ ...DEFAULT_BLEND, ...tiles.blend }),
  };
  const ramp = rampTints(rules, biomeNames);
  for (const name of biomeNames) {
    if (!listsName(tiles, name) || !Object.hasOwn(biomeLayers, name)) continue;
    const layer = biomeLayers[name];
    const texture = `${surfaceOf(name, rules.filter(rule => rule.name === name))}_${layer}`;
    out.textures[name] = texture;
    out.placeholder_colors[texture] = NEUTRAL_PLACEHOLDER;
    out.layers[texture] = layer;
    out.tints[name] = tiles.tints?.[name] ?? ramp[name];
  }
  return out;
}

// The default preset's palette: tints per biome kind, from crater floor to ridge (lower ground
// is darker so craters read as dark bowls). Biome names are \`<kind>_<band>\`.
export const PLANET_BANDS = ['floor', 'low', 'mid', 'upland', 'ridge'];
export const PLANET_PALETTE = {
  snow: ['#C9D3DB', '#D8E0E6', '#E4EAEE', '#EEF2F5', '#F8FAFB'],
  frost: ['#7F8D97', '#93A0A9', '#A7B2B9', '#BAC3C8', '#CDD4D8'],
  plain: ['#57534D', '#6B665E', '#7F7970', '#938C81', '#A79F93'],
  desert: ['#6F5E48', '#85715A', '#9B846A', '#AF977A', '#C1A98B'],
  sulfur: ['#6E6230', '#8A7B3A', '#A49244', '#BBA850', '#CDBC62'],
  basalt: ['#24262A', '#2F3135', '#3A3C40', '#46484B', '#535457'],
};
export const PLANET_NEUTRAL = '#EEEEEE';

// Kind and band of a default-preset biome or texture name, or null.
export function planetName(name) {
  const match = /^(snow|frost|plain|desert|sulfur|basalt)_(floor|low|mid|upland|ridge)$/.exec(name);
  return match ? { kind: match[1], band: PLANET_BANDS.indexOf(match[2]) } : null;
}

// Palette tints for the default preset's biomes (others are left out).
export function planetTints(biomeNames) {
  return Object.fromEntries(biomeNames.map(name => [name, planetName(name)])
    .filter(([, parsed]) => parsed)
    .map(([name, { kind, band }]) => [name, PLANET_PALETTE[kind][band]]));
}

// The default preset's presentation: one neutral texture per biome, overlay layer by height band
// (floor 1 to ridge 5), palette tints. Biomes outside the palette (crater territory) draw their
// ground.
export function planetPresentation(biomeNames, blend = {}) {
  const tiles = { textures: {}, default_placeholder: '#FF00FF', placeholder_colors: {}, layers: {}, tints: planetTints(biomeNames), blend: sanitizeBlend({ ...DEFAULT_BLEND, ...blend }) };
  for (const name of Object.keys(tiles.tints)) {
    tiles.placeholder_colors[name] = PLANET_NEUTRAL;
    tiles.layers[name] = planetName(name).band + 1;
  }
  return tiles;
}

// ---------------------------------------------------------------- images

export function hexRgb(color) {
  const value = String(color || '').replace('#', '');
  const full = value.length === 3 ? [...value].map(c => c + c).join('') : value;
  const rgb = [0, 2, 4].map(i => parseInt(full.slice(i, i + 2), 16));
  return rgb.every(Number.isFinite) && full.length === 6 ? rgb : [255, 0, 255];
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
