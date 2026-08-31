/**
 * IPC handlers for the Codex desktop-app account swap feature. Wires the
 * three `codex:app-*` channels to the typed daemon client in
 * `codex/service/codexAppDaemon.ts`. Mirrors the registration shape in
 * `shell/runtime/notifications.ts`.
 *
 * Each handler funnels a failed daemon `Result` into a thrown `Error` so
 * `handleChannel`'s `ipcMain.handle` rejects the renderer's `invoke()`
 * promise -- this is the fixed `codex:app-*` contract ("throws on
 * failure"); the wire result shape carries no error variant to return
 * instead.
 */
import { apiErrorMessage } from '#result';
import log from '#main/global/providers/logger';
import { handleChannel } from '#main/global/runtime/ipcSecurity';
import { codexAppRestore, codexAppStatus, codexAppUse } from '#main/codex/service/codexAppDaemon';

export function registerCodexAppHandlers(): void {
  handleChannel('codex:app-use', async (_event, label) => {
    const result = await codexAppUse(label);
    if (result.isErr()) {
      const message = apiErrorMessage(result.error);
      log.error(`[codex-app] app-use failed: ${message}`);
      // invariant: this IPC handler funnels a failed daemon Result into a
      // throw so ipcMain.handle rejects the renderer's invoke() promise,
      // matching the fixed codex:app-use contract ("throws on failure").
      throw new Error(message);
    }
  });

  handleChannel('codex:app-restore', async () => {
    const result = await codexAppRestore();
    if (result.isErr()) {
      const message = apiErrorMessage(result.error);
      log.error(`[codex-app] app-restore failed: ${message}`);
      // invariant: this IPC handler funnels a failed daemon Result into a
      // throw so ipcMain.handle rejects the renderer's invoke() promise,
      // matching the fixed codex:app-restore contract ("throws on failure").
      throw new Error(message);
    }
  });

  handleChannel('codex:app-status', async () => {
    const result = await codexAppStatus();
    if (result.isErr()) {
      const message = apiErrorMessage(result.error);
      log.error(`[codex-app] app-status failed: ${message}`);
      // invariant: this IPC handler funnels a failed daemon Result into a
      // throw so ipcMain.handle rejects the renderer's invoke() promise,
      // matching the fixed codex:app-status contract ("throws on failure").
      throw new Error(message);
    }
    return result.value;
  });
}
