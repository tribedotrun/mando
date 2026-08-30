import fs from 'fs';
import path from 'path';
import { shell } from 'electron';
import { handleChannel } from '#main/global/runtime/ipcSecurity';
import log from '#main/global/providers/logger';
import { isSafeExternalUrl } from '#main/shell/service/shellBridge';

function resolveAbsoluteExistingFile(input: string): string | null {
  if (!input || !path.isAbsolute(input)) return null;

  try {
    return fs.statSync(input).isFile() ? input : null;
  } catch {
    return null;
  }
}

export function registerShellBridgeHandlers(): void {
  handleChannel('shell:open-external-url', async (_event, url) => {
    if (!isSafeExternalUrl(url)) throw new Error('Invalid external URL');
    await shell.openExternal(url);
  });

  handleChannel('shell:open-local-path', async (_event, input) => {
    const existing = resolveAbsoluteExistingFile(input);
    if (!existing) throw new Error('Local file does not exist');

    const result = await shell.openPath(existing);
    if (result) {
      log.warn(`Failed to open local path ${existing}: ${result}`);
      throw new Error(result);
    }
  });
}
