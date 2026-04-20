/**
 * Isolates Chromium user-data per data-dir so multiple dev/sandbox
 * instances don't fight over `~/Library/Application Support/Electron/`.
 * Must run before `app.whenReady()` -- Chromium locks the profile on
 * startup.
 *
 * Allowlisted for sync IO: this module's whole job is the sync `mkdirSync`
 * call that Electron expects to happen synchronously during bootstrap.
 */
import { app } from 'electron';
import fs from 'fs';
import path from 'path';
import { getDataDir, getAppMode } from '#main/global/config/lifecycle';

export function isolateChromiumProfile(): void {
  if (getAppMode() === 'production') return;
  const userDataDir = path.join(getDataDir(), 'electron-profile');
  fs.mkdirSync(userDataDir, { recursive: true });
  app.setPath('userData', userDataDir);
}
