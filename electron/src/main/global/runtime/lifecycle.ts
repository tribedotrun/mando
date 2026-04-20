/**
 * Daemon connection management -- health checks, reconnection, version
 * handshake, and daemon startup orchestration.
 *
 * State lives in two typed containers:
 *   - `connection` -- phase + reconnect bookkeeping
 *   - `runtime`    -- timers + quit flag
 */
import { app, dialog } from 'electron';
import log from '#main/global/providers/logger';
import { healthResponseSchema } from '#shared/daemon-contract/schemas';
import { readAppPackageVersion } from '#main/global/runtime/appPackage';
import {
  stageDaemonBinary,
  installDaemonPlist,
  updateDaemonBinary,
  rollbackDaemonBinary,
  kickstartDaemon,
} from '#main/global/runtime/launchd';
import type { ConnectionState } from '#main/global/types/lifecycle';
import {
  POLL_DELAY_MS,
  HEALTH_MONITOR_INTERVAL_MS,
  MAX_RECONNECT_DELAY,
  KICKSTART_AFTER_ATTEMPTS,
  getAppMode,
} from '#main/global/config/lifecycle';
import { compareSemver, getAppTitle, isProcessAlive } from '#main/global/service/lifecycle';
import {
  hasExternalGatewayToken,
  invalidateDiscoveryCache,
} from '#main/global/service/daemonDiscovery';
import {
  killDaemonByPid,
  readExistingDaemonPid,
  hasDaemonConfig,
} from '#main/global/service/daemonProcess';
import { daemonRouteFetch } from '#main/global/runtime/daemonTransport';
import { createDaemonConnectionStore } from '#main/global/runtime/daemonConnectionState';

export {
  readPort,
  readToken,
  invalidateDiscoveryCache,
} from '#main/global/service/daemonDiscovery';
export { daemonRouteFetch, daemonRouteJsonR } from '#main/global/runtime/daemonTransport';
export { daemonRouteSignal } from '#main/global/service/daemonSignal';

const INITIAL_RECONNECT_DELAY_MS = 1000;

const connection = createDaemonConnectionStore({
  initialDelay: INITIAL_RECONNECT_DELAY_MS,
  maxDelay: MAX_RECONNECT_DELAY,
});

interface LifecycleRuntime {
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  healthMonitorInterval: ReturnType<typeof setInterval> | null;
  isQuitting: boolean;
}

const runtime: LifecycleRuntime = {
  reconnectTimer: null,
  healthMonitorInterval: null,
  isQuitting: false,
};

export function setIsQuitting(quitting: boolean): void {
  runtime.isQuitting = quitting;
}

export function updateTrayTooltip(): string {
  const title = getAppTitle(getAppMode());
  const tooltips: Record<ConnectionState, string> = {
    connecting: `${title} — Connecting...`,
    connected: `${title} — Connected`,
    disconnected: `${title} — Disconnected`,
    updating: `${title} — Updating daemon...`,
  };
  return tooltips[connection.phase()];
}

async function healthCheck(): Promise<{ healthy: boolean; version?: string }> {
  try {
    const response = await daemonRouteFetch('getHealth', undefined, {
      signal: AbortSignal.timeout(5000),
    });
    if (response.ok) {
      const raw: unknown = await response.json();
      const parsed = healthResponseSchema.safeParse(raw);
      if (parsed.success) {
        connection.dispatch({ type: 'health_check_ok' });
        return { healthy: parsed.data.healthy, version: parsed.data.version };
      }
      if (connection.get().healthCheckFailureStreak === 0) {
        log.info('healthCheck response failed schema parse', parsed.error.issues);
      } else {
        log.debug('healthCheck response failed schema parse', parsed.error.issues);
      }
      connection.dispatch({ type: 'health_check_failed' });
      return { healthy: false };
    }

    if (connection.get().healthCheckFailureStreak === 0) {
      log.info(`healthCheck: HTTP ${response.status} from daemon`);
    } else {
      log.debug(`healthCheck: HTTP ${response.status} from daemon`);
    }
    connection.dispatch({ type: 'health_check_failed' });
    return { healthy: false };
  } catch (err: unknown) {
    const reason = err instanceof Error ? err.message : String(err);
    if (connection.get().healthCheckFailureStreak === 0) {
      log.info(`healthCheck failed: ${reason}`);
    } else {
      log.debug(`healthCheck failed: ${reason}`);
    }
    connection.dispatch({ type: 'health_check_failed' });
    return { healthy: false };
  }
}

async function waitForDaemon(timeoutMs = 15000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const result = await healthCheck();
    if (result.healthy) {
      connection.dispatch({ type: 'connected' });
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_DELAY_MS));
  }
  return false;
}

function scheduleReconnect(): void {
  if (runtime.reconnectTimer || runtime.isQuitting) return;

  runtime.reconnectTimer = setTimeout(() => {
    void (async () => {
      runtime.reconnectTimer = null;
      invalidateDiscoveryCache();

      const result = await healthCheck();
      if (result.healthy) {
        connection.dispatch({ type: 'connected' });
        return;
      }

      const next = connection.dispatch({ type: 'reconnect_failed' });
      if (
        next.reconnectAttempts === KICKSTART_AFTER_ATTEMPTS &&
        !process.env.MANDO_EXTERNAL_GATEWAY
      ) {
        log.info('[daemon] reconnect attempts exhausted — kickstarting via launchd');
        kickstartDaemon();
      }

      scheduleReconnect();
    })();
  }, connection.get().reconnectDelay);
}

export function startHealthMonitor(): void {
  runtime.healthMonitorInterval = setInterval(() => {
    void (async () => {
      if (runtime.isQuitting || connection.phase() === 'updating') return;

      const result = await healthCheck();
      const phase = connection.phase();
      if (result.healthy && phase !== 'connected') {
        invalidateDiscoveryCache();
        connection.dispatch({ type: 'connected' });
      } else if (!result.healthy && phase === 'connected') {
        invalidateDiscoveryCache();
        connection.dispatch({ type: 'disconnected' });
        scheduleReconnect();
      }
    })();
  }, HEALTH_MONITOR_INTERVAL_MS);
}

async function checkVersionAndUpdate(dataDir: string): Promise<void> {
  const result = await healthCheck();
  if (!result.healthy || !result.version) return;

  const bundledVersion = readAppPackageVersion();
  if (!bundledVersion || compareSemver(bundledVersion, result.version) <= 0) {
    if (bundledVersion && compareSemver(bundledVersion, result.version) < 0) {
      log.info(
        `Daemon ${result.version} is newer than bundled ${bundledVersion} — skipping update`,
      );
    }
    return;
  }

  log.info(`Version mismatch: daemon=${result.version}, bundled=${bundledVersion}. Updating...`);
  connection.dispatch({ type: 'updating' });

  const success = updateDaemonBinary(dataDir);
  if (!success) {
    log.error('Daemon binary update failed');
    connection.dispatch({ type: 'disconnected' });
    return;
  }

  invalidateDiscoveryCache();
  const ready = await waitForDaemon(10000);
  if (ready) return;

  log.error('Updated daemon failed health check, rolling back');
  rollbackDaemonBinary(dataDir);
  invalidateDiscoveryCache();
  await waitForDaemon(10000);
}

export async function ensureDaemon(dataDir: string): Promise<boolean> {
  if (process.env.MANDO_EXTERNAL_GATEWAY) {
    if (!(await hasExternalGatewayToken(dataDir))) {
      connection.dispatch({ type: 'disconnected' });
      scheduleReconnect();
      return false;
    }

    const ready = await waitForDaemon(10000);
    if (!ready) {
      connection.dispatch({ type: 'disconnected' });
      scheduleReconnect();
    }
    return ready;
  }

  const health = await healthCheck();
  if (health.healthy) {
    connection.dispatch({ type: 'connected' });
    await checkVersionAndUpdate(dataDir);
    return true;
  }

  const existingPid = readExistingDaemonPid(dataDir);
  if (existingPid && isProcessAlive(existingPid)) {
    log.info(`Killing existing daemon (pid ${existingPid}) before starting fresh...`);
    const killed = await killDaemonByPid(existingPid, dataDir);
    invalidateDiscoveryCache();
    if (!killed) {
      dialog.showErrorBox(
        'Mando — Cannot Start',
        `Could not stop the existing daemon (PID ${existingPid}).\n\nKill it manually: kill -9 ${existingPid}`,
      );
      app.exit(1);
      return false;
    }
  }

  if (!hasDaemonConfig(dataDir)) {
    return false;
  }

  stageDaemonBinary();
  installDaemonPlist(dataDir);

  const ready = await waitForDaemon(15000);
  if (!ready) {
    connection.dispatch({ type: 'disconnected' });
    scheduleReconnect();
  }
  return ready;
}

export function cleanupDaemon(): void {
  if (runtime.reconnectTimer) {
    clearTimeout(runtime.reconnectTimer);
    runtime.reconnectTimer = null;
  }
  if (runtime.healthMonitorInterval) {
    clearInterval(runtime.healthMonitorInterval);
    runtime.healthMonitorInterval = null;
  }
}
