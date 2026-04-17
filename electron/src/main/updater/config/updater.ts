import { app } from 'electron';
import path from 'path';

export const UPDATE_CHECK_INTERVAL_MS = 30 * 60 * 1000;
export const INITIAL_CHECK_DELAY_MS = 10 * 1000;
export const UPDATE_SERVER = 'https://mando-update.gm-e6e.workers.dev';
export const MAX_REDIRECTS = 5;

export function getStagingDir(): string {
  return path.join(app.getPath('userData'), 'updates');
}

export function getPendingPath(): string {
  return path.join(getStagingDir(), 'pending.json');
}

export function getChannelConfigPath(): string {
  return path.join(app.getPath('userData'), 'update-channel.json');
}

export function getAppBundlePath(): string {
  return path.resolve(process.execPath, '..', '..', '..');
}
