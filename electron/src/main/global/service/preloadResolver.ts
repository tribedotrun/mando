/**
 * Resolves the preload script path. Forge's `.vite/build` layout and the
 * test-build layout place `preload/index.js` at different relative paths
 * from the main-process build directory, so this service probes the
 * known candidates via `fs.existsSync` and returns the first match.
 *
 * Service-tier sync IO is allowlisted under invariant M4 — runtime
 * owners (e.g. windowOwner) call this instead of doing their own fs
 * probing.
 */
import path from 'path';
import fs from 'fs';

export function resolvePreload(mainBuildDir: string): string {
  const candidates = [
    path.join(mainBuildDir, '../preload/index.js'), // test-build layout
    path.join(mainBuildDir, 'preload/index.js'), // forge .vite/build layout
  ];
  return candidates.find((p) => fs.existsSync(p)) ?? candidates[0];
}
