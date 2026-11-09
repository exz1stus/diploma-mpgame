export function temperatureDomain(biomes) {
  if (!biomes.length) return { min: -50, max: 50 };
  return {
    min: Math.min(...biomes.map(biome => biome.temperature.min)),
    max: Math.max(...biomes.map(biome => biome.temperature.max)),
  };
}

export function biomeAt(biomes, temperature, height) {
  return biomes.find(biome => biome.temperature && biome.height
    && temperature >= biome.temperature.min && temperature < biome.temperature.max
    && height >= biome.height.min
    && (height < biome.height.max || (height === 1 && biome.height.max === 1)));
}

export function biomeForSample(biomes, sample) {
  if (![1, 2].includes(sample?.status)) return null;
  return biomes[sample.biomeId] || null;
}

export function tileForSample(palette, sample) {
  return sample?.status === 1 ? palette[sample.tileId] || null : null;
}

// Intersect independent tile mapping rules with a biome classification region.
export function tileRegions(biome, rules, palette) {
  return rules.filter(rule => rule.biome === null || rule.biome === biome.biome).map(rule => ({
    color: palette[rule.tile].color, tileId: rule.tile,
    temperature: {
      min: Math.max(biome.temperature.min, rule.temperature.min),
      max: Math.min(biome.temperature.max, rule.temperature.max),
    },
    height: {
      min: Math.max(biome.height.min, rule.height.min),
      max: Math.min(biome.height.max, rule.height.max),
    },
  })).filter(entry => entry.temperature.min < entry.temperature.max && entry.height.min < entry.height.max);
}

export function biomeDescription(biome) {
  const name = biome.name;
  if (!biome.temperature || !biome.height) return name;
  return `${name} · ${biome.temperature.min} to ${biome.temperature.max} °C · height ${biome.height.min} to ${biome.height.max}`;
}

export function renderBiomes(chart, legend, readout, biomes, palette = [], tileRules = []) {
  const element = (tag, attrs, text) => {
    const node = document.createElementNS('http://www.w3.org/2000/svg', tag);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
    if (text !== undefined) node.textContent = text;
    return node;
  };
  const domain = temperatureDomain(biomes);
  const x = value => 40 + (value - domain.min) / (domain.max - domain.min) * 230;
  const y = value => 186 - value * 160;
  chart.replaceChildren(element('title', {}, 'Biome ranges by temperature and height'));
  chart.append(element('rect', { x: 40, y: 26, width: 230, height: 160, fill: '#362b3d' }));
  // The generator chooses the first matching biome, so draw earlier definitions last.
  for (const biome of [...biomes].reverse()) {
    const fallback = { color: '#52616b', tileId: null,
      height: biome.height, temperature: biome.temperature };
    for (const entry of [fallback, ...tileRegions(biome, tileRules, palette).reverse()]) {
      const description = `${biome.name} · ${entry.tileId === null ? 'No matching tile' : `Tile ${entry.tileId} · ${palette[entry.tileId].name}`} · ${entry.temperature.min} to ${entry.temperature.max} °C · height ${entry.height.min} to ${entry.height.max}`;
      const rect = element('rect', {
        x: x(entry.temperature.min), y: y(entry.height.max),
        width: x(entry.temperature.max) - x(entry.temperature.min),
        height: y(entry.height.min) - y(entry.height.max),
        fill: entry.color, stroke: '#101a2366', 'stroke-width': 0.5,
        'data-biome': biome.name, 'data-tile-id': entry.tileId,
        tabindex: 0, 'aria-label': description,
      });
      rect.append(element('title', {}, description));
      const inspect = () => { readout.textContent = description; };
      rect.addEventListener('pointerenter', inspect);
      rect.addEventListener('focus', inspect);
      chart.append(rect);
    }
  }
  for (const height of [0, 0.25, 0.5, 0.75, 1]) {
    chart.append(element('text', { x: 34, y: y(height) + 3, 'text-anchor': 'end' }, height));
  }
  for (let i = 0; i <= 4; i++) {
    const temperature = domain.min + (domain.max - domain.min) * i / 4;
    chart.append(element('text', { x: x(temperature), y: 202, 'text-anchor': 'middle' }, Number(temperature.toFixed(1))));
  }
  chart.append(element('text', { x: 40, y: 14 }, 'Height (0–1)'));
  chart.append(element('text', { x: 155, y: 223, 'text-anchor': 'middle' }, 'Temperature (°C)'));
  legend.replaceChildren();
  for (const [id, tile] of palette.entries()) {
    const row = document.createElement('span');
    row.title = `Tile ${id} · ${tile.name} · ${tile.color}`;
    const swatch = document.createElement('i');
    swatch.className = 'swatch'; swatch.style.backgroundColor = tile.color;
    row.append(swatch, document.createTextNode(`${id} · ${tile.name}`));
    legend.append(row);
  }
  const unmatched = document.createElement('span');
  const swatch = document.createElement('i');
  swatch.className = 'swatch unclassified';
  unmatched.append(swatch, document.createTextNode('No matching tile'));
  legend.append(unmatched);
  readout.textContent = biomes.length ? 'Hover or focus a rule to inspect its biome and global tile ID. Tile mapping uses final height, temperature and biome. Height 1 is included.' : 'This map has no rules; feature tiles use the default map.';
}
