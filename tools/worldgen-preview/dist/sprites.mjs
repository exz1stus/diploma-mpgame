// Atlas regions cut from crates/worldgen/examples/textures.png (served as /textures.png).
// Rects are [x, y, width, height], trimmed to opaque pixels. Ground textures are 32 × 32
// seamless tiles; one texture spans 2 × 2 world tiles at the native 16 source pixels per tile.
export const ATLAS_URL = './textures.png';
export const SOURCE_TILE_PX = 16;
export const TEXTURE_PX = 32;

// The gray strip is ordered from high to low terrain, left to right: the rockier the texture,
// the lower the ground. Indices follow the sheet: GRAY_0 is the leftmost (highest) texture.
export const SURFACE = Object.freeze({ GRAY_0: 0, GRAY_1: 1, GRAY_2: 2, GRAY_3: 3, BASALT: 4, ICE: 5, SNOW: 6 });
export const SURFACE_NAMES = ['regolith (highland)', 'regolith (cracked)', 'regolith (rocky)', 'regolith (lowland)', 'basalt', 'ice', 'snow'];

// Gray variants are pure luminance and take the full biome tint. Colored materials keep most of
// their own hue so snow, ice, and basalt stay recognizable under every biome color.
export const SURFACE_TEXTURES = [
  { origin: [288, 384], tint: 'luminance' },
  { origin: [320, 384], tint: 'luminance' },
  { origin: [352, 384], tint: 'luminance' },
  { origin: [384, 384], tint: 'luminance' },
  { origin: [384, 256], tint: 'blend', strength: .5 },
  { origin: [416, 128], tint: 'blend', strength: .2 },
  { origin: [384, 128], tint: 'blend', strength: .3 },
];

export const COLD_TEMPERATURE = -12;
// Cold terrain mostly lies between 0.5 and 0.8, so ice covers the lower half of it.
export const SNOW_HEIGHT = 0.6;
// Roughly quartiles of non-frozen terrain height; lower ground gets the rockier texture.
const GRAY_BANDS = [[0.54, SURFACE.GRAY_3], [0.64, SURFACE.GRAY_2], [0.74, SURFACE.GRAY_1]];

// Pebble scatter overlays: the eight 32 × 32 cells of the top gray row, cut by grid.
export const PEBBLE_CELLS = Array.from({ length: 8 }, (_, i) => [i * 32, 384]);
// Overlay probability per 32 px block; lower ground is rockier. None on snow or ice.
export function pebbleChance(height) {
  return Math.min(.45, Math.max(0, (.85 - height) * 1.1));
}

export function isBasaltTile(name) {
  return typeof name === 'string' && /basalt|volcan/.test(name);
}

// Very cold ground freezes regardless of biome: ice in low terrain, snow on high terrain.
export function surfaceFor(basalt, height, temperature) {
  if (temperature < COLD_TEMPERATURE) return height >= SNOW_HEIGHT ? SURFACE.SNOW : SURFACE.ICE;
  if (basalt) return SURFACE.BASALT;
  for (const [limit, surface] of GRAY_BANDS) if (height < limit) return surface;
  return SURFACE.GRAY_0;
}

const sprite = (x, y, width, height) => ({ x, y, width, height });

export const SPRITES = {
  rocks: [[8, 424, 15, 16], [40, 428, 16, 10], [65, 429, 21, 14], [103, 429, 18, 15], [133, 425, 23, 18], [165, 425, 23, 17], [196, 420, 19, 20], [228, 425, 23, 18], [165, 455, 18, 17], [197, 449, 24, 27], [228, 451, 19, 25]].map(r => sprite(...r)),
  spikes: [[2, 447, 27, 32], [36, 447, 22, 32], [65, 447, 28, 32], [100, 447, 23, 32], [130, 448, 22, 31]].map(r => sprite(...r)),
  boulders: [[3, 486, 26, 24], [32, 482, 28, 27], [66, 484, 25, 23], [101, 483, 22, 27], [135, 486, 16, 19], [168, 482, 21, 26], [195, 486, 22, 19], [230, 486, 21, 21]].map(r => sprite(...r)),
  stalagmitesSmall: [[14, 277, 6, 9], [24, 270, 6, 16], [36, 266, 7, 20], [49, 264, 9, 23], [67, 262, 6, 25], [82, 261, 9, 26]].map(r => sprite(...r)),
  stalagmitesLarge: [[134, 259, 16, 27], [164, 256, 21, 32], [197, 241, 19, 44], [258, 234, 26, 53]].map(r => sprite(...r)),
  stalagmitesTall: [[101, 242, 24, 45], [197, 241, 19, 44], [228, 241, 21, 45]].map(r => sprite(...r)),
};

// Gray rocks take biome tints like gray ground; basalt stalagmites keep part of their slate hue.
const ROCK = { tint: 'luminance' };
const STALAGMITE = { tint: 'blend', strength: .55 };
const OBJECT_SPRITES = {
  rock: { normal: ['rocks', ROCK], basalt: ['stalagmitesSmall', STALAGMITE] },
  large_rock: { normal: ['boulders', ROCK], basalt: ['stalagmitesLarge', STALAGMITE] },
  iron_ore: { normal: ['spikes', ROCK], basalt: ['stalagmitesTall', STALAGMITE], ore: true },
};

function hash(...values) {
  let h = 0x811c9dc5;
  for (const value of values) { h = Math.imul(h ^ (value | 0), 0x01000193); h ^= h >>> 15; }
  return Math.imul(h ^ (h >>> 13), 0x5bd1e995) >>> 0;
}

// Stable per instance: the same object picks the same sprite in every chunk response and redraw.
export function objectSprite(typeName, object, surface) {
  const entry = Object.hasOwn(OBJECT_SPRITES, typeName) ? OBJECT_SPRITES[typeName] : null;
  if (!entry) return null;
  const [set, tint] = surface === SURFACE.BASALT ? entry.basalt : entry.normal;
  const choices = SPRITES[set];
  const index = hash(object.id.rule_id, object.id.anchor.x, object.id.anchor.y) % choices.length;
  return { set, index, rect: choices[index], tint, ore: !!entry.ore };
}

// Fixed, recognizable variants for legend icons.
const ICONS = { rock: 0, large_rock: 1, iron_ore: 3 };

export function objectIconSprite(typeName) {
  if (!Object.hasOwn(OBJECT_SPRITES, typeName)) return null;
  const { normal: [set, tint], ore } = OBJECT_SPRITES[typeName];
  const index = ICONS[typeName] ?? 0;
  return { set, index, rect: SPRITES[set][index], tint, ore: !!ore };
}

export { hash as spriteHash };
