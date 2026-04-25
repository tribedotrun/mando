import log from '#main/global/providers/logger';
import { POLL_DELAY_MS } from '#main/global/config/lifecycle';
import { daemonRouteJsonR } from '#main/global/runtime/daemonTransport';
import type { DaemonConnectionStore } from '#main/global/runtime/daemonConnectionState';

// invariant: daemon health helpers are the typed bridge between lifecycle orchestration and daemon transport.
export async function healthCheck(
  connection: DaemonConnectionStore,
): Promise<{ healthy: boolean; version?: string }> {
  try {
    const result = await daemonRouteJsonR('getHealth', undefined, {
      signal: AbortSignal.timeout(5000),
    }).toPromise();
    if (result.isOk()) {
      connection.dispatch({ type: 'health_check_ok' });
      return { healthy: result.value.healthy, version: result.value.version };
    }
    if (result.error.code === 'parse') {
      if (connection.get().healthCheckFailureStreak === 0) {
        log.info('healthCheck response failed schema parse', result.error.issues);
      } else {
        log.debug('healthCheck response failed schema parse', result.error.issues);
      }
    } else if (result.error.code === 'http') {
      if (connection.get().healthCheckFailureStreak === 0) {
        log.info(`healthCheck: HTTP ${result.error.status} from daemon`);
      } else {
        log.debug(`healthCheck: HTTP ${result.error.status} from daemon`);
      }
    } else {
      const reason =
        result.error.code === 'network'
          ? result.error.cause
          : result.error.code === 'timeout'
            ? `timeout after ${result.error.ms}ms`
            : result.error.code;
      if (connection.get().healthCheckFailureStreak === 0) {
        log.info(`healthCheck failed: ${reason}`);
      } else {
        log.debug(`healthCheck failed: ${reason}`);
      }
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

// invariant: polling helper returns bare Promise<boolean> because lifecycle owns the state-machine transition side effects.
export async function waitForDaemon(
  connection: DaemonConnectionStore,
  timeoutMs = 15000,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const result = await healthCheck(connection);
    if (result.healthy) {
      connection.dispatch({ type: 'connected' });
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_DELAY_MS));
  }
  return false;
}
