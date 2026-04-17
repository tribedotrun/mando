import fs from 'fs';
import path from 'path';
import { shell } from 'electron';
import { handleTrusted } from '#main/global/runtime/ipcSecurity';
import log from '#main/global/providers/logger';
import { isSafeExternalUrl, expandPath } from '#main/shell/service/terminalBridge';

function resolveExistingFile(input: string, cwd: string): string | null {
  if (!input) return null;

  try {
    const absolute = expandPath(input, cwd);
    return fs.statSync(absolute).isFile() ? absolute : null;
  } catch {
    return null;
  }
}

function resolveAbsoluteExistingFile(input: string): string | null {
  if (!input || !path.isAbsolute(input)) return null;

  try {
    return fs.statSync(input).isFile() ? input : null;
  } catch {
    return null;
  }
}

export function registerTerminalBridgeHandlers(): void {
  handleTrusted('terminal:open-external-url', async (_event, url: string) => {
    if (!isSafeExternalUrl(url)) throw new Error('Invalid external URL');
    await shell.openExternal(url);
  });

  handleTrusted('terminal:resolve-local-path', (_event, input: string, cwd: string) =>
    resolveExistingFile(input, cwd),
  );

  handleTrusted('terminal:open-local-path', async (_event, input: string) => {
    const existing = resolveAbsoluteExistingFile(input);
    if (!existing) throw new Error('Local file does not exist');

    const result = await shell.openPath(existing);
    if (result) {
      log.warn(`Failed to open local path ${existing}: ${result}`);
      throw new Error(result);
    }
  });
}
