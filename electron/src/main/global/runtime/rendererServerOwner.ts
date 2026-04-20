/**
 * Owner for the renderer Vite dev server / static server. Holds the
 * `http.Server` handle and exposes start/stop operations.
 *
 * Codifies invariants M1 and M2 in .claude/skills/s-arch/invariants.md.
 */
import type http from 'http';
import path from 'path';
import log from '#main/global/providers/logger';
import { startRendererServer } from '#main/daemon/runtime/rendererServer';
import { setRenderer } from '#main/global/runtime/windowOwner';

interface RendererServerRuntime {
  server: http.Server | null;
}

const runtime: RendererServerRuntime = { server: null };

const DEV_SERVER_PROBE_TIMEOUT_MS = 1500;

async function devServerReachable(url: string): Promise<boolean> {
  try {
    // eslint-disable-next-line mando/no-direct-fetch -- reason: HEAD probe to localhost vite dev server, not a daemon API call. No body to parse.
    const response = await fetch(url, {
      method: 'HEAD',
      signal: AbortSignal.timeout(DEV_SERVER_PROBE_TIMEOUT_MS),
    });
    return response.ok;
  } catch {
    return false;
  }
}

export async function prepareRendererSource(opts: {
  mainBuildDir: string;
  viteDevServerUrl: string | null | undefined;
  viteName: string;
}): Promise<void> {
  if (opts.viteDevServerUrl && (await devServerReachable(opts.viteDevServerUrl))) {
    setRenderer(opts.viteDevServerUrl, 0);
    return;
  }

  if (opts.viteDevServerUrl) {
    log.warn(
      `[renderer] Vite dev server unavailable at ${opts.viteDevServerUrl}; falling back to static renderer`,
    );
  }

  const rendererDir = path.join(opts.mainBuildDir, `../renderer/${opts.viteName}`);
  const result = await startRendererServer(rendererDir);
  runtime.server = result.server;
  setRenderer(`http://127.0.0.1:${result.port}/index.html`, result.port);
}

export function stopRendererServer(): void {
  runtime.server?.close();
  runtime.server = null;
}
