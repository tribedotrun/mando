/**
 * Resolves packaged / dev asset paths for the main process. The sync
 * filesystem probing is intentional: asset resolution has to be
 * synchronous so the first caller (tray icon install, dock icon set) can
 * hand the path straight to Electron APIs.
 */
import { app } from 'electron';
import fs from 'fs';
import path from 'path';

export function resolveAsset(baseDir: string, name: string): string {
  const candidates = app.isPackaged
    ? [path.join(process.resourcesPath!, 'assets', name)]
    : [
        path.join(app.getAppPath(), 'assets', name),
        path.resolve(baseDir, '../../assets', name),
        path.resolve(baseDir, '../assets', name),
      ];
  return candidates.find((p) => fs.existsSync(p)) ?? candidates[0];
}
