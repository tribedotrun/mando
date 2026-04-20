/**
 * One-off migration: move legacy top-level `startAtLogin` into
 * `ui.openAtLogin`. Keeps migration logic local to Electron startup so the
 * daemon stays free of one-off config upgrade logic. Delete once old
 * configs are no longer in use.
 *
 * Allowlisted for sync IO: runs once during bootstrap before the daemon
 * config HTTP flow is up.
 */
import { app } from 'electron';
import fs from 'fs';
import { z } from 'zod';
import log from '#main/global/providers/logger';
import { getConfigPath } from '#main/global/config/lifecycle';
import { parseJsonText } from '#result';

// Local schema for the login-item migration. The full mando config lives
// in the daemon (`mandoConfigSchema` in `shared/daemon-contract`); this
// targets only the legacy fields the migration touches. Inline keeps the
// migration self-contained.
const loginItemMigrationSchema = z.object({
  startAtLogin: z.boolean().optional(),
  ui: z.object({ openAtLogin: z.boolean().optional() }).passthrough().optional(),
});

export function runLoginItemMigration(): void {
  if (!app.isPackaged) return;

  try {
    const raw = fs.readFileSync(getConfigPath(), 'utf-8');
    const rawConfig = parseJsonText(raw, 'file:login-item-migration');
    if (rawConfig.isErr()) {
      log.warn('[main] login-item config migration skipped: JSON parse failed');
      return;
    }
    const rawJson = rawConfig.value;
    const parsed = loginItemMigrationSchema.safeParse(rawJson);
    if (!parsed.success) {
      log.warn('[main] login-item config migration skipped: schema parse failed');
      return;
    }
    const cfg = parsed.data;
    if (cfg.startAtLogin === undefined || cfg.ui?.openAtLogin !== undefined) return;

    const migrated = cfg.startAtLogin;
    app.setLoginItemSettings({ openAtLogin: migrated, openAsHidden: true });
    // Re-parse the original raw JSON to preserve any unknown fields when re-writing.
    const fullCfg =
      rawJson && typeof rawJson === 'object' ? { ...(rawJson as Record<string, unknown>) } : {};
    fullCfg.ui = {
      ...((fullCfg.ui as Record<string, unknown>) ?? {}),
      openAtLogin: migrated,
    };
    delete fullCfg.startAtLogin;
    fs.writeFileSync(getConfigPath(), JSON.stringify(fullCfg, null, 2), 'utf-8');
  } catch (err: unknown) {
    const code = (err as NodeJS.ErrnoException)?.code;
    if (code === 'ENOENT') {
      // Fresh install, nothing to migrate.
      return;
    }
    log.error('[main] login-item config migration failed:', err);
  }
}
