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

export function surfaceForSample(biomes, sample) {
  return sample?.status === 1 ? biomes[sample.surfaceBiomeId] || null : null;
}

export function paletteIndexAt(biome, temperature, height) {
  if (!biome.palette.length) return null;
  if (!Number.isFinite(temperature) || !Number.isFinite(height) || height < 0 || height > 1) return 0;
  const index = biome.palette.findIndex(entry =>
    (!entry.temperature || (temperature >= entry.temperature.min && temperature < entry.temperature.max))
    && (!entry.height || (height >= entry.height.min
      && (height < entry.height.max || (height === 1 && entry.height.max === 1)))));
  return index < 0 ? 0 : index;
}

// Clip palette rules to the selected biome-map region for an accurate chart.
export function paletteRegions(biome) {
  return biome.palette.map((entry, paletteIndex) => ({
    color: entry.color, paletteIndex,
    temperature: {
      min: Math.max(biome.temperature.min, entry.temperature?.min ?? -Infinity),
      max: Math.min(biome.temperature.max, entry.temperature?.max ?? Infinity),
    },
    height: {
      min: Math.max(biome.height.min, entry.height?.min ?? 0),
      max: Math.min(biome.height.max, entry.height?.max ?? 1),
    },
  })).filter(entry => entry.temperature.min < entry.temperature.max && entry.height.min < entry.height.max);
}

export function biomeDescription(biome) {
  const name = biome.palette.length ? `${biome.name} · ${biome.palette.length} palette ${biome.palette.length === 1 ? 'entry' : 'entries'}${biome.palette_height_threshold ? ` · minimum uplift ${biome.palette_height_threshold}` : ''}` : `${biome.name} · terrain appearance`;
  if (!biome.temperature || !biome.height) return name;
  return `${name} · ${biome.temperature.min} to ${biome.temperature.max} °C · height ${biome.height.min} to ${biome.height.max}`;
}

export function renderBiomes(chart, legend, readout, biomes, repository = biomes) {
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
    // Entry 0 is also the fallback for uncovered palette ranges.
    const fallback = { color: biome.palette[0]?.color || '#52616b', paletteIndex: 0,
      height: biome.height, temperature: biome.temperature };
    for (const entry of [fallback, ...paletteRegions(biome).reverse()]) {
      const description = `${biome.name} · ${biome.palette.length ? `palette entry ${entry.paletteIndex + 1}` : 'terrain appearance'} · ${entry.temperature.min} to ${entry.temperature.max} °C · height ${entry.height.min} to ${entry.height.max}`;
      const rect = element('rect', {
        x: x(entry.temperature.min), y: y(entry.height.max),
        width: x(entry.temperature.max) - x(entry.temperature.min),
        height: y(entry.height.min) - y(entry.height.max),
        fill: entry.color, stroke: '#101a2366', 'stroke-width': 0.5,
        'data-biome': biome.name, 'data-palette-index': entry.paletteIndex,
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
  for (const biome of repository) {
    const row = document.createElement('span');
    row.title = biomeDescription(biome);
    for (const [index, entry] of biome.palette.entries()) {
      const swatch = document.createElement('i');
      swatch.className = 'swatch'; swatch.style.backgroundColor = entry.color;
      swatch.title = `${biome.name} · palette entry ${index + 1} · ${entry.color}`;
      row.append(swatch);
    }
    if (!biome.palette.length) {
      const swatch = document.createElement('i');
      swatch.className = 'swatch unclassified';
      row.append(swatch);
    }
    row.append(document.createTextNode(biome.palette.length ? biome.name : `${biome.name} (terrain appearance)`));
    legend.append(row);
  }
  const unmatched = document.createElement('span');
  const swatch = document.createElement('i');
  swatch.className = 'swatch unclassified';
  unmatched.append(swatch, document.createTextNode('No matching biome'));
  legend.append(unmatched);
  readout.textContent = biomes.length ? 'Hover or focus a rule to inspect its ranges. Territories without palettes use the terrain appearance. Height 1 is included.' : 'This map has no rules; feature tiles use the default map.';
}
