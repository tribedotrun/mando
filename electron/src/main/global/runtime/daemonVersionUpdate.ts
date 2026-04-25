import log from '#main/global/providers/logger';
import { readAppPackageVersion } from '#main/global/runtime/appPackage';
import { rollbackDaemonBinary, updateDaemonBinary } from '#main/global/runtime/launchd';
import { compareSemver } from '#main/global/service/lifecycle';
import { invalidateDiscoveryCache } from '#main/global/service/daemonDiscovery';
import type { DaemonConnectionStore } from '#main/global/runtime/daemonConnectionState';
import { healthCheck, waitForDaemon } from '#main/global/runtime/daemonHealth';

export async function checkVersionAndUpdate(
  dataDir: string,
  connection: DaemonConnectionStore,
): Promise<void> {
  const result = await healthCheck(connection);
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
  const ready = await waitForDaemon(connection, 10000);
  if (ready) return;

  log.error('Updated daemon failed health check, rolling back');
  rollbackDaemonBinary(dataDir);
  invalidateDiscoveryCache();
  await waitForDaemon(connection, 10000);
}
