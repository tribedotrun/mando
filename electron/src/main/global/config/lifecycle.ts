import { app } from 'electron';
import path from 'path';
import os from 'os';
import type { AppMode } from '#main/global/types/lifecycle';

// Timing constants for daemon connection management
export const POLL_DELAY_MS = 200;
export const SIGTERM_POLL_MS = 250;
export const SIGKILL_SETTLE_MS = 500;
export const HEALTH_MONITOR_INTERVAL_MS = 10_000;

// Reconnection backoff
export const MAX_RECONNECT_DELAY = 30000;
export const KICKSTART_AFTER_ATTEMPTS = 3;

export function getAppMode(): AppMode {
  const mode = process.env.MANDO_APP_MODE;
  if (mode === 'dev' || mode === 'sandbox' || mode === 'prod-local' || mode === 'preview')
    return mode;
  if (app.isPackaged && process.execPath.includes('Mando (Preview)')) return 'preview';
  return 'production';
}

export function isHeadless(): boolean {
  if (process.env.MANDO_HEADLESS === '1') return true;
  return getAppMode() === 'sandbox' && process.env.MANDO_SANDBOX_VISIBLE !== '1';
}

export function getDataDir(): string {
  if (process.env.MANDO_DATA_DIR) return process.env.MANDO_DATA_DIR;
  if (getAppMode() === 'preview') return path.join(os.homedir(), '.mando-preview');
  if (getAppMode() === 'dev') return path.join(os.homedir(), '.mando-dev');
  return path.join(os.homedir(), '.mando');
}

export function getConfigPath(): string {
  return process.env.MANDO_CONFIG || path.join(getDataDir(), 'config.json');
}

export const INITIAL_RECONNECT_DELAY_MS = 1000;
