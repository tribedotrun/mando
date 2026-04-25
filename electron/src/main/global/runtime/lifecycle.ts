/**
 * Daemon connection management -- health checks, reconnection, version
 * handshake, and daemon startup orchestration.
 */
import { app, dialog } from 'electron';
import log from '#main/global/providers/logger';
import {
  stageDaemonBinary,
  installDaemonPlist,
  kickstartDaemon,
} from '#main/global/runtime/launchd';
import type { ConnectionState } from '#main/global/types/lifecycle';
import {
  HEALTH_MONITOR_INTERVAL_MS,
  INITIAL_RECONNECT_DELAY_MS,
  KICKSTART_AFTER_ATTEMPTS,
  MAX_RECONNECT_DELAY,
  getAppMode,
} from '#main/global/config/lifecycle';
import { getAppTitle, isProcessAlive } from '#main/global/service/lifecycle';
import {
  hasExternalGatewayToken,
  invalidateDiscoveryCache,
} from '#main/global/service/daemonDiscovery';
import {
  hasDaemonConfig,
  killDaemonByPid,
  readExistingDaemonPid,
} from '#main/global/service/daemonProcess';
import { createDaemonConnectionStore } from '#main/global/runtime/daemonConnectionState';
import { healthCheck, waitForDaemon } from '#main/global/runtime/daemonHealth';
import { checkVersionAndUpdate } from '#main/global/runtime/daemonVersionUpdate';

export {
  readPort,
  readToken,
  invalidateDiscoveryCache,
} from '#main/global/service/daemonDiscovery';
export { daemonRouteFetch, daemonRouteJsonR } from '#main/global/runtime/daemonTransport';
export { daemonRouteSignal } from '#main/global/service/daemonSignal';

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

function scheduleReconnect(): void {
  if (runtime.reconnectTimer || runtime.isQuitting) return;

  runtime.reconnectTimer = setTimeout(() => {
    void (async () => {
      runtime.reconnectTimer = null;
      invalidateDiscoveryCache();

      const result = await healthCheck(connection);
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

      const result = await healthCheck(connection);
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

export async function ensureDaemon(dataDir: string): Promise<boolean> {
  if (process.env.MANDO_EXTERNAL_GATEWAY) {
    if (!(await hasExternalGatewayToken(dataDir))) {
      connection.dispatch({ type: 'disconnected' });
      scheduleReconnect();
      return false;
    }

    const ready = await waitForDaemon(connection, 10000);
    if (!ready) {
      connection.dispatch({ type: 'disconnected' });
      scheduleReconnect();
    }
    return ready;
  }

  const health = await healthCheck(connection);
  if (health.healthy) {
    connection.dispatch({ type: 'connected' });
    await checkVersionAndUpdate(dataDir, connection);
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

  const ready = await waitForDaemon(connection, 15000);
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
