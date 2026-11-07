export function temperatureDomain(biomes) {
  if (!biomes.length) return { min: -50, max: 50 };
  return {
    min: Math.min(...biomes.map(biome => biome.temperature.min)),
    max: Math.max(...biomes.map(biome => biome.temperature.max)),
  };
}

export function biomeAt(biomes, temperature, height) {
  return biomes.find(biome => temperature >= biome.temperature.min && temperature < biome.temperature.max
    && height >= biome.height.min && height < biome.height.max);
}

export function biomeForSample(biomes, sample) {
  if (sample?.status !== 1) return null;
  // Wire metrics are f32; use the generator color to resolve rounding at a boundary.
  const matches = biomes.filter(biome => normalizeColor(biome.color) === sample.color.toLowerCase());
  return biomeAt(matches, sample.temperature, sample.height) || matches[0] || null;
}

function normalizeColor(color) {
  return (color.length === 4 ? `#${[...color.slice(1)].map(channel => channel + channel).join('')}` : color).toLowerCase();
}

export function biomeDescription(biome) {
  return `${biome.name} · ${biome.temperature.min} to ${biome.temperature.max} °C · height ${biome.height.min} to ${biome.height.max}`;
}

export function renderBiomes(chart, legend, readout, biomes) {
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
    const rect = element('rect', {
      x: x(biome.temperature.min), y: y(biome.height.max),
      width: x(biome.temperature.max) - x(biome.temperature.min),
      height: y(biome.height.min) - y(biome.height.max),
      fill: biome.color, stroke: '#101a2366', 'stroke-width': 0.5,
      tabindex: 0, 'aria-label': biomeDescription(biome),
    });
    rect.append(element('title', {}, biomeDescription(biome)));
    const inspect = () => { readout.textContent = biomeDescription(biome); };
    rect.addEventListener('pointerenter', inspect);
    rect.addEventListener('focus', inspect);
    chart.append(rect);
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
  for (const biome of biomes) {
    const row = document.createElement('span');
    row.title = biomeDescription(biome);
    const swatch = document.createElement('i');
    swatch.className = 'swatch'; swatch.style.backgroundColor = biome.color;
    row.append(swatch, document.createTextNode(biome.name));
    legend.append(row);
  }
  const unmatched = document.createElement('span');
  const swatch = document.createElement('i');
  swatch.className = 'swatch unclassified';
  unmatched.append(swatch, document.createTextNode('No matching biome'));
  legend.append(unmatched);
  readout.textContent = biomes.length ? 'Hover or focus a biome to inspect its ranges. Upper bounds are exclusive.' : 'This file has no biome definitions.';
}
