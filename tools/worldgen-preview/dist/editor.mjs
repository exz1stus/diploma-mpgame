// Presentation editor: edits a copy of the client's tiles.ron / objects.ron and texture images,
// renders them through the client renderer, and exports the files for crates/client/assets.
import {
  BLEND_CONTROLS, DEFAULT_BLEND, sanitizeBlend,
} from './terrain.mjs';
import {
  DEFAULT_OBJECT_COLORS, OBJECT_SOURCES, TILE_SOURCES, TINTS, averageColor, bakeObjectTexture, bakeTileTexture,
  bandedPresentation, defaultObjectSource, defaultTileSource, listsName, objectsRon, parseRon, placeholderOf,
  stackLayers, textureCatalog, textureFor, tileCatalog, tilesRon,
} from './presentation.mjs';

// v2: tints and height shading; v1 drafts (per-biome colored textures) are dropped.
const DRAFT_KEY = 'worldgen-preview-presentation-v2';
const SHEET_URL = './textures.png';
const CLIENT = './client-assets';
const GROUND = 'ground';

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

async function pngBase64(image) {
  const blob = await new Promise(resolve => toCanvas(image).toBlob(resolve, 'image/png'));
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

export class PresentationEditor {
  constructor(panel, onChange) {
    this.panel = panel;
    this.onChange = onChange;
    this.state = null;
    this.sheet = null;
    this.files = { tiles: new Map(), objects: new Map() };
    this.meta = null;
    this.look = null;
    this.revision = 0;
    this.timer = 0;
    this.status = '';
  }

  // meta: { biomes: [{name}], maps: { Default: rules, ... }, objectTypes: [{name, footprint}] }
  async setMeta(meta) {
    const first = !this.meta;
    this.meta = meta;
    if (first) {
      try { this.sheet = await loadImageData(SHEET_URL); } catch { this.status = 'Sprite sheet unavailable: sheet sources draw as placeholders.'; }
      await this.loadClientFiles();
      let draft = null;
      try { draft = JSON.parse(localStorage.getItem(DRAFT_KEY)); } catch { /* storage unavailable */ }
      this.state = draft?.tiles && draft?.objects && draft?.sources ? draft : await this.clientState();
    }
    this.rebuild();
  }

  async loadClientFiles() {
    this.files = { tiles: new Map(), objects: new Map() };
    try {
      const index = await (await fetch(`${CLIENT}/index.json`, { cache: 'no-store' })).json();
      for (const folder of ['tiles', 'objects']) {
        await Promise.all((index[folder] || []).map(async name => {
          try { this.files[folder].set(name, await loadImageData(`${CLIENT}/textures/${folder}/${encodeURIComponent(name)}.png`)); } catch { /* unreadable file */ }
        }));
      }
    } catch { /* no dev server listing */ }
  }

  // The client's current files, with defaults for what they leave out: layers from the biome
  // maps, and sheet textures for textures that have no file yet.
  async clientState() {
    const read = async (name, fallback) => {
      try {
        const response = await fetch(`${CLIENT}/world/${name}`, { cache: 'no-store' });
        if (!response.ok) throw new Error(`${response.status}`);
        return parseRon(await response.text());
      } catch (error) {
        this.status = `Cannot read client ${name} (${error.message}); starting from defaults.`;
        return fallback;
      }
    };
    let tiles = await read('tiles.ron', { textures: {}, default_placeholder: '#FF00FF', placeholder_colors: {} });
    const objects = await read('objects.ron', { textures: {}, default_placeholder: '#475A47', placeholder_colors: {} });
    tiles.textures ||= {}; tiles.placeholder_colors ||= {};
    objects.textures ||= {}; objects.placeholder_colors ||= {};
    const { Default: terrain = [], ...features } = this.meta.maps;
    const biomeNames = this.biomeNames();
    // Files from before tints colored each biome's texture; move that color into a tint and
    // share neutral textures per height band.
    if (!tiles.tints) tiles = bandedPresentation(tiles, terrain, Object.values(features), biomeNames);
    tiles.layers ||= {};
    tiles.blend = sanitizeBlend({ ...DEFAULT_BLEND, ...tiles.blend });
    const sources = { tiles: {}, objects: {} };
    const catalog = tileCatalog(biomeNames, tiles, name => this.files.tiles.has(name));
    catalog.textures.forEach((texture, index) => {
      if (this.files.tiles.has(texture.name)) { sources.tiles[texture.name] = { id: 'file' }; return; }
      const users = biomeNames.filter((_, biome) => catalog.textureOf[biome] === index && catalog.ownTiles[biome]);
      const source = users.length && defaultTileSource(texture.name);
      if (source) sources.tiles[texture.name] = source;
    });
    for (const type of this.meta.objectTypes) {
      const texture = textureFor(objects, type.name);
      if (!Object.hasOwn(objects.placeholder_colors, texture) && DEFAULT_OBJECT_COLORS[texture]) objects.placeholder_colors[texture] = DEFAULT_OBJECT_COLORS[texture];
      const source = this.files.objects.has(texture) ? { id: 'file' } : defaultObjectSource(texture);
      if (source) sources.objects[texture] = source;
    }
    return { tiles, objects, sources };
  }

  biomeNames() { return this.meta.biomes.map(biome => biome.name); }

  tileImage(name) {
    const source = this.state.sources.tiles[name];
    if (!source) return null;
    if (source.id === 'file') return this.files.tiles.get(name) || null;
    return this.sheet ? bakeTileTexture(this.sheet, source, placeholderOf(this.state.tiles, name)) : null;
  }

  objectImage(name, footprint) {
    const source = this.state.sources.objects[name];
    if (!source) return null;
    if (source.id === 'file') return this.files.objects.get(name) || null;
    const pixels = sanitizeBlend(this.state.tiles.blend).pixels_per_tile;
    return this.sheet ? bakeObjectTexture(this.sheet, source, placeholderOf(this.state.objects, name), footprint, pixels) : null;
  }

  // What the renderer needs: the client's catalogs and texture array for the current state.
  buildLook() {
    const { tiles, objects } = this.state;
    const biomeNames = this.biomeNames();
    const images = new Map();
    const tileImage = name => {
      if (!images.has(name)) images.set(name, this.tileImage(name));
      return images.get(name);
    };
    const catalog = tileCatalog(biomeNames, tiles, name => !!tileImage(name));
    const stack = stackLayers(catalog.textures, catalog.textures.map(texture => tileImage(texture.name)));
    const colors = new Uint8Array(catalog.textures.length * 3);
    stack.layers.forEach((layer, i) => colors.set(averageColor({ width: stack.width, height: stack.height, data: layer }), i * 3));
    const objectCatalog = textureCatalog(this.meta.objectTypes.map(type => type.name), objects);
    const objectLooks = this.meta.objectTypes.map((type, kind) => {
      const texture = objectCatalog.textures[objectCatalog.textureOf[kind]];
      const image = this.objectImage(texture.name, type.footprint);
      return { texture: texture.name, color: texture.placeholder, image: image && toCanvas(image) };
    });
    return {
      revision: ++this.revision,
      catalog, stack, objectLooks,
      blend: sanitizeBlend(tiles.blend),
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
    try { localStorage.setItem(DRAFT_KEY, JSON.stringify(this.state)); } catch { /* storage unavailable */ }
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.look = this.buildLook();
      if (rerender) this.render(); else this.renderStatus();
      this.onChange(this.look);
    }, 120);
  }

  async resetToClient() {
    try { localStorage.removeItem(DRAFT_KEY); } catch { /* storage unavailable */ }
    await this.loadClientFiles();
    this.state = await this.clientState();
    this.rebuild();
  }

  // ------------------------------------------------------------ export

  async exportFiles() {
    const { tiles, objects } = this.state;
    const files = [
      { path: 'world/tiles.ron', text: tilesRon(tiles) },
      { path: 'world/objects.ron', text: objectsRon(objects) },
    ];
    const tileNames = new Set(this.look.catalog.textures.map(texture => texture.name));
    for (const name of tileNames) {
      const image = this.tileImage(name);
      if (image) files.push({ path: `textures/tiles/${name}.png`, base64: await pngBase64(image) });
    }
    for (const [kind, look] of this.look.objectLooks.entries()) {
      if (!look.image || files.some(file => file.path === `textures/objects/${look.texture}.png`)) continue;
      const image = this.objectImage(look.texture, this.meta.objectTypes[kind].footprint);
      files.push({ path: `textures/objects/${look.texture}.png`, base64: await pngBase64(image) });
    }
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

  sourceSelect(folder, name, ids, hasFile) {
    const select = element('select', { className: 'source-select', title: 'Texture source' });
    const options = [['', 'Placeholder color'], ...(hasFile ? [['file', 'Client file']] : []), ...ids.map(id => [id, `Sheet: ${id.replaceAll('_', ' ')}`])];
    select.append(...options.map(([value, text]) => element('option', { value, textContent: text })));
    const source = this.state.sources[folder][name];
    select.value = source?.id && (source.id !== 'file' || hasFile) ? source.id : '';
    return select;
  }

  tintControls(folder, name) {
    const source = this.state.sources[folder][name];
    const select = element('select', { className: 'tint-select', title: 'Color the sheet art with the placeholder color' });
    select.append(...TINTS.map(tint => element('option', { value: tint, textContent: tint })));
    select.value = source?.tint || 'none';
    const strength = element('input', { type: 'number', min: 0, max: 1, step: 0.05, value: source?.strength ?? 1, className: 'strength', title: 'Tint strength' });
    const sheetSource = source && source.id !== 'file';
    select.disabled = strength.disabled = !sheetSource;
    const apply = () => {
      const current = this.state.sources[folder][name];
      if (!current) return;
      current.tint = select.value;
      current.strength = Math.min(1, Math.max(0, Number(strength.value) || 0));
      this.changed();
    };
    select.addEventListener('change', apply);
    strength.addEventListener('change', apply);
    return [select, strength];
  }

  thumbnail(image, color) {
    const canvas = element('canvas', { width: 24, height: 24, className: 'thumb' });
    const context = canvas.getContext('2d');
    context.imageSmoothingEnabled = false;
    if (image) context.drawImage(image instanceof HTMLCanvasElement ? image : toCanvas(image), 0, 0, 24, 24);
    else { context.fillStyle = color; context.fillRect(0, 0, 24, 24); }
    return canvas;
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

    const textureRows = catalog.textures.map((texture, index) => {
      const users = biomeNames.filter((_, biome) => catalog.textureOf[biome] === index && catalog.ownTiles[biome]);
      const color = element('input', { type: 'color', value: texture.placeholder.slice(0, 7), title: 'Placeholder color (drawn while no texture file exists; tints sheet art)' });
      color.addEventListener('change', () => { tiles.placeholder_colors[texture.name] = color.value.toUpperCase(); this.changed(); });
      const layer = element('input', { type: 'number', min: 0, max: 255, step: 1, value: catalog.overlay[index], className: 'layer', title: 'Overlay layer: higher layers overlay lower ones; equal layers fade' });
      layer.addEventListener('change', () => { tiles.layers[texture.name] = Math.min(255, Math.max(0, Math.round(Number(layer.value) || 0))); this.changed({ rerender: false }); });
      const source = this.sourceSelect('tiles', texture.name, Object.keys(TILE_SOURCES), this.files.tiles.has(texture.name));
      source.addEventListener('change', () => {
        if (!source.value) delete this.state.sources.tiles[texture.name];
        else this.state.sources.tiles[texture.name] = { tint: 'tint', strength: 1, ...this.state.sources.tiles[texture.name], id: source.value };
        // A texture with a source must be listed, or its biomes would draw their ground.
        if (!listsName(tiles, texture.name)) tiles.placeholder_colors[texture.name] = texture.placeholder;
        this.changed();
      });
      const image = this.look.stack.layers[index] && this.tileImage(texture.name);
      return element('div', { className: 'texture-row' }, [
        this.thumbnail(image, texture.placeholder), color,
        element('span', { className: 'texture-name', textContent: texture.name, title: users.length ? `Biomes: ${users.join(', ')}` : 'Only drawn as ground' }),
        layer, source, ...this.tintControls('tiles', texture.name),
      ]);
    });

    const names = [...new Set([...catalog.textures.map(texture => texture.name), ...Object.keys(tiles.placeholder_colors)])].sort();
    const datalist = element('datalist', { id: 'texture-names' }, [GROUND, ...names].map(name => element('option', { value: name })));
    const biomeRows = biomeNames.map((name, biome) => {
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
          delete this.state.sources.tiles[name];
        } else {
          if (value && value !== name) tiles.textures[name] = value;
          const texture = value || name;
          if (!Object.hasOwn(tiles.placeholder_colors, texture)) tiles.placeholder_colors[texture] = tiles.default_placeholder;
        }
        this.changed();
      });
      const tint = element('input', { type: 'color', value: (tiles.tints?.[name] ?? '#FFFFFF').slice(0, 7), title: 'Tint: the ground is multiplied by this color, smoothed across neighboring tiles' });
      tint.addEventListener('change', () => { tiles.tints ||= {}; tiles.tints[name] = tint.value.toUpperCase(); this.changed({ rerender: false }); });
      return element('label', { className: 'biome-row' }, [element('span', { textContent: name }), tint, input]);
    });

    const objectCatalog = textureCatalog(this.meta.objectTypes.map(type => type.name), objects);
    const objectRows = objectCatalog.textures.map((texture, index) => {
      const kinds = this.meta.objectTypes.filter((_, kind) => objectCatalog.textureOf[kind] === index);
      const color = element('input', { type: 'color', value: texture.placeholder.slice(0, 7), title: 'Placeholder color' });
      color.addEventListener('change', () => { objects.placeholder_colors[texture.name] = color.value.toUpperCase(); this.changed(); });
      const source = this.sourceSelect('objects', texture.name, Object.keys(OBJECT_SOURCES), this.files.objects.has(texture.name));
      source.addEventListener('change', () => {
        if (!source.value) delete this.state.sources.objects[texture.name];
        else this.state.sources.objects[texture.name] = { tint: 'tint', strength: 1, ...this.state.sources.objects[texture.name], id: source.value };
        this.changed();
      });
      const look = this.look.objectLooks[this.meta.objectTypes.indexOf(kinds[0])];
      return element('div', { className: 'texture-row' }, [
        this.thumbnail(look?.image, texture.placeholder), color,
        element('span', { className: 'texture-name', textContent: texture.name, title: kinds.map(kind => `${kind.name} ${kind.footprint.width}×${kind.footprint.height}`).join(', ') }),
        source, ...this.tintControls('objects', texture.name),
      ]);
    });

    const save = element('button', { type: 'button', textContent: 'Save client files' });
    const reload = element('button', { type: 'button', textContent: 'Reload client files' });
    this.statusLine = element('p', { className: 'settings-hint export-status' });
    save.addEventListener('click', async () => {
      save.disabled = true;
      this.statusLine.textContent = 'Exporting…';
      try {
        const { directory, files } = await this.exportFiles();
        this.status = `Saved ${files.length} files to ${directory}. Copy its contents into crates/client/assets/.`;
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
    this.panel.replaceChildren(
      details('Tile blending', isOpen('Tile blending', true), [blend],
        'Client renderer settings (tiles.ron blend). Height: on different layers the higher texture overlays the lower one with a crisp edge and shades its foot. Same layer: bilinear blend. Temperature: biome tints, smoothed across tiles.'),
      details(`Tile textures (${catalog.textures.length})`, isOpen(`Tile textures (${catalog.textures.length})`, true), textureRows,
        'Each row: placeholder color, texture, overlay layer (height); then source, bake tint and strength. Keep textures neutral and shared per height band; biome tints color them. Sprite-sheet sources are baked and exported as PNGs.'),
      details('Biomes: tint and texture', isOpen('Biomes: tint and texture', true), [datalist, ...biomeRows],
        `Tint (temperature color, smoothed across tiles) and texture per biome. Blank texture uses the biome's own name; several biomes may share one. "${GROUND}" draws the terrain biome underneath.`),
      details('Object textures', isOpen('Object textures', true), objectRows,
        'Drawn stretched over the footprint, as in the client. Sheet sprites are baked at footprint × pixels per tile, standing on the bottom edge.'),
      element('div', { className: 'action-row' }, [save, reload]),
      this.statusLine,
    );
    this.renderStatus();
  }
}

