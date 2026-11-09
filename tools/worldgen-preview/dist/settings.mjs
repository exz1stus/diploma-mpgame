export function defaultSettings(controls) {
  return Object.fromEntries(controls.map(control => [control.key, control.default]));
}

export function settingsError(config, controls) {
  for (const control of controls) {
    const value = config[control.key];
    if (value === null && control.default === null) continue;
    if (!Number.isFinite(value) || value < control.min || value > control.max
      || (control.step === '1' && !Number.isInteger(value))) {
      return `${control.label}: enter ${control.step === '1' ? 'an integer' : 'a number'} from ${control.min} to ${control.max}.`;
    }
  }
  const keys = new Set(controls.map(control => control.key));
  for (const [prefix, name, influence] of [['crater', 'crater', 1.5], ['basin', 'basin', 1.5], ['volcano', 'volcano', 1]]) {
    if ([`${prefix}_min_radius`, `${prefix}_max_radius`, `${prefix}_cell_size`].every(key => keys.has(key))) {
      if (config[`${prefix}_min_radius`] > config[`${prefix}_max_radius`]) return `Minimum ${name} radius must not exceed maximum radius.`;
      if (config[`${prefix}_max_radius`] * influence > config[`${prefix}_cell_size`]) return `Maximum ${name} radius${influence === 1 ? '' : ` × ${influence}`} must fit within the cell size.`;
    }
  }
  if (keys.has('volcano_min_height') && keys.has('volcano_max_height') && config.volcano_min_height > config.volcano_max_height) {
    return 'Minimum volcano height must not exceed maximum height.';
  }
  for (const [suffix, label] of [['depth', 'depth'], ['rim_height', 'rim height']]) {
    const min = `basin_min_${suffix}`;
    const max = `basin_max_${suffix}`;
    if (keys.has(min) && keys.has(max) && config[min] > config[max]) return `Minimum basin ${label} must not exceed maximum ${label}.`;
  }
  return '';
}

export function readSettings(params, controls) {
  const config = defaultSettings(controls);
  for (const control of controls) {
    const key = control.key === 'size' ? 'chunk' : control.key;
    if (!params.has(key)) continue;
    const raw = params.get(key).trim();
    const value = raw === '' ? (control.default === null ? null : NaN) : Number(raw);
    const candidate = { ...config, [control.key]: value };
    if (!settingsError(candidate, [control])) config[control.key] = value;
  }
  for (const prefix of ['crater_', 'basin_', 'volcano_']) {
    const group = controls.filter(control => control.key.startsWith(prefix));
    if (settingsError(config, group)) {
      for (const control of group) config[control.key] = control.default;
    }
  }
  return config;
}

export function settingsQuery(config, controls, forUrl = false) {
  const query = new URLSearchParams();
  for (const { key } of controls) {
    if (config[key] !== null) query.set(forUrl && key === 'size' ? 'chunk' : key, config[key]);
  }
  return query;
}
