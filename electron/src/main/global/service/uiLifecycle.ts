const KEEP_ENV_KEYS = new Set([
  'MANDO_APP_MODE',
  'MANDO_DATA_DIR',
  'MANDO_EXTERNAL_GATEWAY',
  'MANDO_GATEWAY_PORT',
  'MANDO_LOG_DIR',
  'MANDO_HEADLESS',
  'MANDO_SANDBOX_VISIBLE',
  'ELECTRON_DISABLE_SECURITY_WARNINGS',
  'VITE_DEV_SERVER_URL',
]);

export function uiLaunchEnv(): Record<string, string> {
  return Object.entries(process.env).reduce<Record<string, string>>((env, [key, value]) => {
    if (KEEP_ENV_KEYS.has(key) && typeof value === 'string' && value.length > 0) {
      env[key] = value;
    }
    return env;
  }, {});
}
