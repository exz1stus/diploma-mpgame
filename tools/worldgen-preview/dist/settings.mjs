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
  return config;
}

export function settingsQuery(config, controls, forUrl = false) {
  const query = new URLSearchParams();
  for (const { key } of controls) {
    if (config[key] !== null) query.set(forUrl && key === 'size' ? 'chunk' : key, config[key]);
  }
  return query;
}
