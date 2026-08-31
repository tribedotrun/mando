/**
 * Typed daemon client for ChatGPT desktop-app Codex account swaps. The daemon
 * owns credential storage, slot files, recovery copies, and macOS process
 * handling; Electron only forwards the three IPC operations.
 */
import type { CodexDesktopAppStatusResponse } from '#shared/daemon-contract';
import { daemonRouteJsonR } from '#main/global/runtime/daemonTransport';
import type { ApiError, Result } from '#result';

function codexHomeOverride(): string | undefined {
  const value = process.env.CODEX_HOME?.trim();
  return value || undefined;
}

export async function codexAppUse(label: string): Promise<Result<void, ApiError>> {
  const result = await daemonRouteJsonR('postCredentialsCodexAppUse', undefined, {
    body: { label, codexHome: codexHomeOverride() },
  });
  return result.map(() => undefined);
}

export async function codexAppRestore(): Promise<Result<void, ApiError>> {
  const result = await daemonRouteJsonR('postCredentialsCodexAppRestore', undefined, {
    body: { codexHome: codexHomeOverride() },
  });
  return result.map(() => undefined);
}

export async function codexAppStatus(): Promise<Result<CodexDesktopAppStatusResponse, ApiError>> {
  return daemonRouteJsonR('getCredentialsCodexAppStatus', {
    query: { codexHome: codexHomeOverride() },
  });
}
