// Presentation editor: edits a copy of the client's tiles.ron / objects.ron, renders them with
// the client's atlas (world/atlas.ron and its sheet, read-only here), and exports the two files
// for crates/client/assets.
import {
  BLEND_CONTROLS, DEFAULT_BLEND, sanitizeBlend,
} from './terrain.mjs';
import {
  averageColor, bandedPresentation, cropRegion, listsName, migrateObjects, objectBiomeTint, objectPlaceholder, objectStyleOf,
  objectSurface, objectsRon, parseRon, placeholderOf, planetTints, rampTints, readAtlas, recolorImage, recolorRegions, regionFor, spriteFor,
  spriteRects, spriteVariants,
  stackLayers, tileCatalog, tileRegion, tilesRon,
} from './presentation.mjs';
import { fnv1a, styleJitter, styleScale } from './placement.mjs';

// v6: object tints per biome; older drafts are dropped.
const DRAFT_KEY = 'worldgen-preview-presentation-v6';
const CLIENT = './client-assets';
// The client's own presentation; presets live in world/presets/<name>/ and fall back to it
// file by file (atlas.ron, tiles.ron, objects.ron).
const WORLD = 'world';
const CURRENT = '';
const GROUND = 'ground';
const LAYERS = ['Sorted', 'Ground'];

const element = (tag, props = {}, children = []) => {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
};

function loadImageData(url) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      const canvas = element('canvas', { width: image.naturalWidth, height: image.naturalHeight });
      const context = canvas.getContext('2d', { willReadFrequently: true });
      context.drawImage(image, 0, 0);
      resolve(context.getImageData(0, 0, canvas.width, canvas.height));
    };
    image.onerror = () => reject(new Error(`Cannot load ${url}`));
    image.src = url;
  });
}

function toCanvas(image) {
  const canvas = element('canvas', { width: image.width, height: image.height });
  canvas.getContext('2d').putImageData(new ImageData(Uint8ClampedArray.from(image.data), image.width, image.height), 0, 0);
  return canvas;
}

export class PresentationEditor {
  constructor(panel, onChange) {
    this.panel = panel;
    this.onChange = onChange;
    this.state = null;
    this.atlas = null;
    this.sheet = null;
    this.sheetCanvas = null;
    this.meta = null;
    this.look = null;
    this.revision = 0;
    this.timer = 0;
    this.status = '';
    // Presentation presets found in the client's world/presets/, and the one being edited.
    this.presets = null;
    this.presentation = CURRENT;
  }

  // Each presentation edits its own client files and keeps its own draft.
  worldPath() { return this.presentation ? `${WORLD}/presets/${this.presentation}` : WORLD; }
  draftKey() { return `${DRAFT_KEY}:${this.presentation || WORLD}`; }

  // meta: { preset, biomes: [{name}], maps: { Default: rules, ... }, objectTypes: [{name, footprint}] }
  // A new worldgen preset selects the presentation preset of the same name, if there is one.
  async setMeta(meta) {
    const reload = !this.meta || meta.preset !== this.meta.preset;
    this.meta = meta;
    if (!reload) { this.rebuild(); return; }
    this.presets ??= await this.listPresets();
    const matching = String(meta.preset ?? '').replace(/\.ron$/, '');
    await this.selectPresentation(this.presets.includes(matching) ? matching : CURRENT);
  }

  async listPresets() {
    try {
      return (await (await fetch(`${CLIENT}/presets.json`, { cache: 'no-store' })).json()).presets ?? [];
    } catch {
      return [];
    }
  }

  async selectPresentation(name) {
    this.presentation = name;
    this.status = '';
    await this.loadAtlas();
    let draft = null;
    try { draft = JSON.parse(localStorage.getItem(this.draftKey())); } catch { /* storage unavailable */ }
    this.state = draft?.tiles && draft?.objects ? draft : await this.clientState();
    this.rebuild();
  }

  // A client file of the selected presentation, else of the client's own world/ folder.
  async fetchClient(name) {
    for (const folder of new Set([this.worldPath(), WORLD])) {
      const response = await fetch(`${CLIENT}/${folder}/${name}`, { cache: 'no-store' });
      if (response.ok) return response.text();
    }
    throw new Error('not found');
  }

  async loadAtlas() {
    this.atlas = null;
    this.sheet = this.sheetCanvas = null;
    try {
      this.atlas = readAtlas(await this.fetchClient('atlas.ron'));
      this.sheet = await loadImageData(`${CLIENT}/${this.atlas.image}`);
      this.sheetCanvas = toCanvas(this.sheet);
    } catch (error) {
      this.atlas ||= { image: '', tiles: {}, sprites: {} };
      this.status = `Client atlas unavailable (${error.message}): art draws as placeholders.`;
    }
  }

  async readClient(name, fallback) {
    try {
      return parseRon(await this.fetchClient(name));
    } catch (error) {
      this.status = `Cannot read client ${name} (${error.message}); starting from defaults.`;
      return fallback;
    }
  }

  // The client's current files, with defaults for what they leave out.
  async clientState() {
    let tiles = await this.readClient('tiles.ron', { textures: {}, default_placeholder: '#FF00FF', placeholder_colors: {} });
    const objects = migrateObjects(await this.readClient('objects.ron', { default_placeholder: '#475A47' }));
    tiles.textures ||= {}; tiles.placeholder_colors ||= {};
    const { Default: terrain = [], ...features } = this.meta.maps;
    // Files from before tints colored each biome's texture; move that color into a tint and
    // share neutral textures per height band.
    if (!tiles.tints) tiles = bandedPresentation(tiles, terrain, Object.values(features), this.biomeNames());
    tiles.regions ||= {};
    tiles.layers ||= {};
    tiles.blend = sanitizeBlend({ ...DEFAULT_BLEND, ...tiles.blend });
    return { tiles, objects };
  }

  biomeNames() { return this.meta.biomes.map(biome => biome.name); }

  tileImage(name) {
    const { tiles } = this.state;
    const rect = tileRegion(this.atlas, tiles, name);
    const image = rect ? cropRegion(this.sheet, rect) : null;
    return image && tiles.recolor ? recolorImage(image, placeholderOf(tiles, name)) : image;
  }

  // The first variant of a sprite, for thumbnails.
  spriteImage(name) {
    const variants = spriteVariants(this.atlas, name);
    return variants ? cropRegion(this.sheet, variants[0].rect) : null;
  }

  // The sheet objects draw from: the atlas sheet, or a copy with every sprite recolored
  // (objects.ron recolor), kept until the color or the sheet changes.
  objectSheet(recolor) {
    if (!recolor || !this.sheet) return this.sheetCanvas;
    if (this.recolored?.sheet !== this.sheet || this.recolored.color !== recolor) {
      this.recolored = { sheet: this.sheet, color: recolor, canvas: toCanvas(recolorRegions(this.sheet, spriteRects(this.atlas), recolor)) };
    }
    return this.recolored.canvas;
  }

  // What the renderer needs: the client's catalogs and texture array, and how objects draw.
  buildLook() {
    const { tiles, objects } = this.state;
    const biomeNames = this.biomeNames();
    const images = new Map();
    const tileImage = name => {
      if (!images.has(name)) images.set(name, this.tileImage(name));
      return images.get(name);
    };
    const catalog = tileCatalog(biomeNames, tiles, name => !!tileRegion(this.atlas, tiles, name));
    const stack = stackLayers(catalog.textures, catalog.textures.map(texture => tileImage(texture.name)));
    const colors = new Uint8Array(catalog.textures.length * 3);
    stack.layers.forEach((layer, i) => colors.set(averageColor({ width: stack.width, height: stack.height, data: layer }), i * 3));
    const blend = sanitizeBlend(tiles.blend);
    return {
      revision: ++this.revision,
      catalog, stack, blend,
      objects: {
        sheet: this.sheetCanvas,
        recolored: this.objectSheet(objects.recolor),
        atlas: this.atlas,
        tilesPerPx: 1 / blend.pixels_per_tile,
        kinds: this.meta.objectTypes.map(type => {
          const style = objectStyleOf(objects, type.name);
          return { name: type.name, style, salt: fnv1a(type.name), placeholder: objectPlaceholder(objects, style) };
        }),
        surface: (biome, ground) => objectSurface(objects, biomeNames, biome, ground),
        tint: (biome, ground) => objectBiomeTint(objects, biomeNames, biome, ground),
      },
      palette: {
        own: Uint8Array.from(catalog.ownTiles, Number),
        textureOf: Uint16Array.from(catalog.textureOf),
        colors,
        tints: Uint8Array.from(catalog.tints.flat()),
      },
    };
  }

  rebuild() {
    this.look = this.buildLook();
    this.render();
    this.onChange(this.look);
  }

  changed({ rerender = true } = {}) {
    try { localStorage.setItem(this.draftKey(), JSON.stringify(this.state)); } catch { /* storage unavailable */ }
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.look = this.buildLook();
      if (rerender) this.render(); else this.renderStatus();
      this.onChange(this.look);
    }, 120);
  }

  async resetToClient() {
    try { localStorage.removeItem(this.draftKey()); } catch { /* storage unavailable */ }
    this.presets = await this.listPresets();
    await this.selectPresentation(this.presets.includes(this.presentation) ? this.presentation : CURRENT);
  }

  // Picks which client presentation (world/ or a world/presets/ folder) is drawn and edited.
  presentationSelect() {
    const select = element('select', { title: 'Client presentation: crates/client/assets/world/ or a preset folder in world/presets/ (its missing files come from world/)' },
      [[CURRENT, 'Current (world/)'], ...(this.presets ?? []).map(name => [name, `Preset: ${name}`])]
        .map(([value, textContent]) => element('option', { value, textContent })));
    select.value = this.presentation;
    select.addEventListener('change', () => this.selectPresentation(select.value));
    return element('label', { className: 'field' }, [element('span', { textContent: 'Presentation' }), select]);
  }

  // ------------------------------------------------------------ export

  async exportFiles() {
    const { tiles, objects } = this.state;
    const files = [
      { path: 'world/tiles.ron', text: tilesRon(tiles) },
      { path: 'world/objects.ron', text: objectsRon(objects) },
    ];
    const response = await fetch('./export', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ files }) });
    if (!response.ok) throw new Error(await response.text());
    return response.json();
  }

  // ------------------------------------------------------------ UI

  renderStatus() {
    const warnings = this.look?.stack.warnings || [];
    this.statusLine.textContent = [this.status, ...warnings].filter(Boolean).join(' · ');
  }

  numberField(label, value, { min, max, step }, apply) {
    const input = element('input', { type: 'number', min, max, step, value });
    input.addEventListener('change', () => {
      const number = Number(input.value);
      if (Number.isFinite(number)) apply(number);
    });
    return element('label', { className: 'field' }, [element('span', { textContent: label }), input]);
  }

  // A select of atlas names; `none` labels the empty choice.
  atlasSelect(names, value, none, title) {
    const select = element('select', { className: 'source-select', title });
    select.append(...[['', none], ...names.sort().map(name => [name, name.replaceAll('_', ' ')])]
      .map(([option, text]) => element('option', { value: option, textContent: text })));
    select.value = names.includes(value) ? value : '';
    return select;
  }

  // Art scaled to fit 24 × 24, keeping its proportions.
  thumbnail(image, color) {
    const canvas = element('canvas', { width: 24, height: 24, className: 'thumb' });
    const context = canvas.getContext('2d');
    context.imageSmoothingEnabled = false;
    if (image) {
      const scale = Math.min(24 / image.width, 24 / image.height);
      const width = image.width * scale;
      const height = image.height * scale;
      context.drawImage(toCanvas(image), (24 - width) / 2, 24 - height, width, height);
    } else { context.fillStyle = color; context.fillRect(0, 0, 24, 24); }
    return canvas;
  }

  tileRows(tiles, catalog, biomeNames) {
    const regionNames = Object.keys(this.atlas.tiles);
    return catalog.textures.map((texture, index) => {
      const users = biomeNames.filter((_, biome) => catalog.textureOf[biome] === index && catalog.ownTiles[biome]);
      const color = element('input', { type: 'color', value: texture.placeholder.slice(0, 7), title: 'Placeholder color (drawn while the texture has no atlas region)' });
      color.addEventListener('change', () => { tiles.placeholder_colors[texture.name] = color.value.toUpperCase(); this.changed(); });
      const layer = element('input', { type: 'number', min: 0, max: 255, step: 1, value: catalog.overlay[index], className: 'layer', title: 'Overlay layer: higher layers overlay lower ones' });
      layer.addEventListener('change', () => { tiles.layers[texture.name] = Math.min(255, Math.max(0, Math.round(Number(layer.value) || 0))); this.changed({ rerender: false }); });
      const region = this.atlasSelect(regionNames, regionFor(tiles, texture.name), 'Placeholder color', 'Atlas tile region (tiles.ron regions)');
      region.addEventListener('change', () => {
        if (region.value === texture.name) delete tiles.regions[texture.name];
        else tiles.regions[texture.name] = region.value;
        // A texture with art must be listed, or its biomes would draw their ground.
        if (!listsName(tiles, texture.name)) tiles.placeholder_colors[texture.name] = texture.placeholder;
        this.changed();
      });
      return element('div', { className: 'texture-row' }, [
        this.thumbnail(this.tileImage(texture.name), texture.placeholder), color,
        element('span', { className: 'texture-name', textContent: texture.name, title: users.length ? `Biomes: ${users.join(', ')}` : 'Only drawn as ground' }),
        layer, region,
      ]);
    });
  }

  biomeRows(tiles, objects, catalog, biomeNames) {
    const names = [...new Set([...catalog.textures.map(texture => texture.name), ...Object.keys(tiles.placeholder_colors)])].sort();
    const surfaces = [...new Set(Object.values(objects.surfaces))].sort();
    const lists = [
      element('datalist', { id: 'texture-names' }, [GROUND, ...names].map(name => element('option', { value: name }))),
      element('datalist', { id: 'surface-names' }, surfaces.map(name => element('option', { value: name }))),
    ];
    const rows = biomeNames.map((name, biome) => {
      const own = catalog.ownTiles[biome];
      const input = element('input', {
        value: !own ? GROUND : Object.hasOwn(tiles.textures, name) ? tiles.textures[name] : '',
        placeholder: name, className: 'biome-texture',
        title: `Blank: texture "${name}". "${GROUND}": draw the terrain biome underneath (feature biomes).`,
      });
      input.setAttribute('list', 'texture-names');
      input.addEventListener('change', () => {
        const value = input.value.trim();
        delete tiles.textures[name];
        if (value === GROUND) {
          delete tiles.placeholder_colors[name];
          delete tiles.regions[name];
        } else {
          if (value && value !== name) tiles.textures[name] = value;
          const texture = value || name;
          if (!Object.hasOwn(tiles.placeholder_colors, texture)) tiles.placeholder_colors[texture] = tiles.default_placeholder;
        }
        this.changed();
      });
      const tint = element('input', { type: 'color', value: (tiles.tints?.[name] ?? '#FFFFFF').slice(0, 7), title: 'Tint: the ground is multiplied by this color, smoothed across neighboring tiles' });
      tint.addEventListener('change', () => { tiles.tints ||= {}; tiles.tints[name] = tint.value.toUpperCase(); this.changed({ rerender: false }); });
      const surface = element('input', { value: objects.surfaces[name] ?? '', placeholder: 'surface', className: 'biome-surface', title: 'Surface (objects.ron surfaces): which per-surface object art stands on this biome' });
      surface.setAttribute('list', 'surface-names');
      surface.addEventListener('change', () => {
        const value = surface.value.trim();
        if (value) objects.surfaces[name] = value; else delete objects.surfaces[name];
        this.changed();
      });
      const objectTint = element('input', { type: 'color', value: (objects.tints?.[name] ?? '#FFFFFF').slice(0, 7), title: 'Object tint (objects.ron tints): tinted objects standing on this biome are multiplied by it' });
      objectTint.addEventListener('change', () => { objects.tints ||= {}; objects.tints[name] = objectTint.value.toUpperCase(); this.changed({ rerender: false }); });
      return element('label', { className: 'biome-row' }, [element('span', { textContent: name }), tint, input, surface, objectTint]);
    });
    return [...lists, ...rows];
  }

  // objects.ron recolor (object tints are set per biome in the biome rows).
  objectColorFields(objects) {
    const recolorOn = element('input', { type: 'checkbox', checked: !!objects.recolor, title: 'Make object art neutral before tints' });
    const recolor = element('input', { type: 'color', value: (objects.recolor || '#EEEEEE').slice(0, 7), disabled: !objects.recolor, title: 'Neutral color of recolored object art' });
    const apply = () => {
      objects.recolor = recolorOn.checked ? recolor.value.toUpperCase() : null;
      recolor.disabled = !recolorOn.checked;
      this.changed({ rerender: false });
    };
    recolorOn.addEventListener('change', apply);
    recolor.addEventListener('change', apply);
    return element('div', { className: 'object-fields' }, [
      element('label', { textContent: 'recolor tinted art ' }, [recolorOn, recolor]),
    ]);
  }

  objectRows(objects) {
    const spriteNames = Object.keys(this.atlas.sprites);
    const surfaces = [...new Set(Object.values(objects.surfaces))].sort();
    return this.meta.objectTypes.map((type, kind) => {
      const style = () => (objects.kinds[type.name] ||= {});
      const current = objectStyleOf(objects, type.name);
      const placeholder = objectPlaceholder(objects, current);
      const color = element('input', { type: 'color', value: placeholder.slice(0, 7), title: 'Placeholder color (drawn while the kind has no sprite)' });
      color.addEventListener('change', () => { style().placeholder = color.value.toUpperCase(); this.changed(); });
      const sprite = this.atlasSelect(spriteNames, current.sprite ?? '', `${type.name} (kind name)`, 'Atlas sprite where no surface sprite is set');
      sprite.addEventListener('change', () => {
        if (sprite.value) style().sprite = sprite.value; else delete style().sprite;
        this.changed();
      });
      const jitter = element('input', { type: 'number', min: 0, max: 0.5, step: 0.05, value: styleJitter(current), title: 'Jitter: largest nudge off center, in tiles' });
      jitter.addEventListener('change', () => { style().jitter = styleJitter({ jitter: Number(jitter.value) }); this.changed({ rerender: false }); });
      const flip = element('input', { type: 'checkbox', checked: !!current.flip, title: 'Mirror half of the objects; only for art without painted light' });
      const tinted = element('input', { type: 'checkbox', checked: current.tinted !== false, title: 'Take the object colors (recolor and biome object tints); off keeps the art in its own colors' });
      tinted.addEventListener('change', () => {
        if (tinted.checked) delete style().tinted; else style().tinted = false;
        this.changed({ rerender: false });
      });
      flip.addEventListener('change', () => { style().flip = flip.checked; this.changed({ rerender: false }); });
      const layer = element('select', { title: 'Sorted: stands, drawn by foot position. Ground: flat decal.' },
        LAYERS.map(value => element('option', { value, textContent: value })));
      layer.value = current.layer === 'Ground' ? 'Ground' : 'Sorted';
      layer.addEventListener('change', () => { style().layer = layer.value; this.changed({ rerender: false }); });
      const [min, max] = styleScale(current);
      const scale = [min, max].map((value, i) => {
        const input = element('input', { type: 'number', min: 0.1, max: 10, step: 0.05, value, title: i ? 'Largest scale' : 'Smallest scale' });
        input.addEventListener('change', () => {
          const next = styleScale(style());
          next[i] = Number(input.value);
          style().scale = styleScale({ scale: next });
          this.changed({ rerender: false });
        });
        return input;
      });
      const surfaceSprites = surfaces.map(surface => {
        const select = this.atlasSelect(spriteNames, current.sprites?.[surface] ?? '', 'default sprite', `Sprite on ${surface}`);
        select.addEventListener('change', () => {
          style().sprites ||= {};
          if (select.value) style().sprites[surface] = select.value; else delete style().sprites[surface];
          this.changed();
        });
        return element('label', { className: 'surface-sprite' }, [element('span', { textContent: surface }), select]);
      });
      return element('div', { className: 'object-row' }, [
        this.thumbnail(this.spriteImage(spriteFor(current, type.name, null)), placeholder), color,
        element('span', { className: 'texture-name', textContent: type.name, title: `${type.footprint.width}×${type.footprint.height} footprint · kind ${kind}` }),
        sprite,
        element('span', { className: 'object-fields' }, [
          element('label', { textContent: 'jitter ' }, [jitter]), element('label', { textContent: 'flip ' }, [flip]), element('label', { textContent: 'tinted ' }, [tinted]), layer,
          element('label', { textContent: 'scale ' }, scale),
        ]),
        element('div', { className: 'surface-sprites' }, surfaceSprites),
      ]);
    });
  }

  render() {
    if (!this.state || !this.look) return;
    const { tiles, objects } = this.state;
    const { catalog } = this.look;
    const biomeNames = this.biomeNames();

    const blend = element('div', { className: 'blend-fields' }, BLEND_CONTROLS.map(control =>
      this.numberField(control.label, sanitizeBlend(tiles.blend)[control.key], control, value => {
        tiles.blend = sanitizeBlend({ ...tiles.blend, [control.key]: value });
        this.changed();
      })));
    const recolor = element('input', { type: 'checkbox', checked: tiles.recolor === true });
    recolor.addEventListener('change', () => { tiles.recolor = recolor.checked; this.changed(); });
    const recolorField = element('label', { className: 'field', title: 'tiles.ron recolor: make tile art neutral in its placeholder color (light and shade only), so tints alone color the ground' },
      [element('span', { textContent: 'Neutral tile art (recolor)' }), recolor]);

    // Palette biomes get the preset palette; any others the temperature ramp.
    const rampButton = element('button', { type: 'button', textContent: 'Reset tints to palette' });
    rampButton.addEventListener('click', () => {
      const rules = Object.values(this.meta.maps).flat();
      const own = biomeNames.filter((_, biome) => catalog.ownTiles[biome]);
      tiles.tints = { ...tiles.tints, ...rampTints(rules, own), ...planetTints(own) };
      this.changed();
    });

    const save = element('button', { type: 'button', textContent: 'Save client files' });
    const reload = element('button', { type: 'button', textContent: 'Reload client files' });
    this.statusLine = element('p', { className: 'settings-hint export-status' });
    save.addEventListener('click', async () => {
      save.disabled = true;
      this.statusLine.textContent = 'Exporting…';
      try {
        const { directory, files } = await this.exportFiles();
        const target = this.worldPath() === WORLD ? 'crates/client/assets/' : `crates/client/assets/ (world/*.ron go to ${this.worldPath()}/)`;
        this.status = `Saved ${files.length} files to ${directory}. Copy its contents into ${target}.`;
      } catch (error) {
        this.status = `Export failed: ${error.message}`;
      }
      save.disabled = false;
      this.renderStatus();
    });
    reload.addEventListener('click', () => {
      if (confirm('Discard edits and reload crates/client/assets?')) { this.status = ''; this.resetToClient(); }
    });

    const details = (title, open, children, hint) => {
      const node = element('details', { className: 'settings-group', open }, [element('summary', { textContent: title })]);
      if (hint) node.append(element('p', { className: 'settings-hint', textContent: hint }));
      node.append(...children);
      return node;
    };
    const opened = new Set([...this.panel.querySelectorAll('details[open] > summary')].map(summary => summary.textContent));
    const rendered = this.rendered;
    this.rendered = true;
    const isOpen = (title, fallback) => (rendered ? opened.has(title) : fallback);
    const textureTitle = `Tile textures (${catalog.textures.length})`;
    this.panel.replaceChildren(
      this.presentationSelect(),
      details('Tile blending', isOpen('Tile blending', true), [blend, recolorField],
        'Client renderer settings (tiles.ron blend). Height: on different layers the higher texture overlays the lower one with a crisp edge and shades its foot. Biomes within a layer meet on crisp, noisy edges too. Tints color each biome; a tint radius above 0 softens colors inside a biome. Pixels per tile also sets the size objects are drawn at.'),
      details(textureTitle, isOpen(textureTitle, true), this.tileRows(tiles, catalog, biomeNames),
        'Each row: placeholder color, texture, overlay layer (height) and its atlas tile region. Art comes from the client atlas (world/atlas.ron), which is edited by hand.'),
      details('Biomes: tint, texture and surface', isOpen('Biomes: tint, texture and surface', true),
        [element('div', { className: 'action-row' }, [rampButton]), ...this.biomeRows(tiles, objects, catalog, biomeNames)],
        `Ground tint (temperature color), texture, object surface and object tint per biome. Blank texture uses the biome's own name; "${GROUND}" draws the terrain biome underneath. The surface picks per-surface object art; the object tint colors tinted objects there, apart from the ground so they stand out.`),
      details('Objects', isOpen('Objects', true), [this.objectColorFields(objects), ...this.objectRows(objects)],
        'Ground tint multiplies objects by the tint of the biome they stand on; recolor first makes their art neutral in a color. Each kind: placeholder color, default sprite, jitter (tiles), flip, layer, scale range, and a sprite per surface. One variant per object is picked from its anchor tile, as in the client.'),
      element('div', { className: 'action-row' }, [save, reload]),
      this.statusLine,
    );
    this.renderStatus();
  }
}
