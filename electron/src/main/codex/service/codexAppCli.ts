/**
 * Shells out to the bundled `mando` CLI for Codex desktop-app (ChatGPT/Codex
 * Electron app) account-swap operations. This stays a pure `service/` file
 * per the tier architecture: it returns typed `Result`s instead of logging
 * or throwing. `codex/runtime/codexApp.ts` is the sole caller; it logs
 * failures and converts them into IPC handler throws.
 */
import { execFile } from 'child_process';
import { promisify } from 'util';
import type { z } from 'zod';
import { getDataDir } from '#main/global/config/lifecycle';
import { cliSourcePath, currentPath } from '#main/global/service/launchd';
import { codexAppStatusSchema } from '#shared/ipc-contract';
import { type ApiError, type Result, err, ioError, ok, parseJsonTextWith } from '#result';

const execFileAsync = promisify(execFile);
// The CLI can spend up to ~12s waiting for the desktop ChatGPT/Codex app to
// quit, then relaunch it and round-trip with the daemon to pick and sync the
// swapped account. 15s was tight enough to SIGTERM the child mid-swap,
// leaving the desktop app closed or the slot/state partially written. 120s
// gives that quit + relaunch + daemon round-trip budget real headroom;
// `app-status` finishes in well under a second, so one generous constant is
// fine for all three subcommands.
const CLI_TIMEOUT_MS = 120_000;

async function runMandoCli(args: string[]): Promise<Result<string, ApiError>> {
  // Spawn the CLI bundled with the running app, not `cliInstallPath()`'s
  // staged copy -- auto-update does not re-run `copyCliBinary()`, so the
  // staged copy can be an older version missing `codex app-*` entirely.
  const cmd = cliSourcePath();
  try {
    const { stdout } = await execFileAsync(cmd, args, {
      encoding: 'utf-8',
      timeout: CLI_TIMEOUT_MS,
      env: {
        ...process.env,
        PATH: currentPath(),
        // Preview/dev builds must operate on their own credential pool and
        // swap state, never the production one the bare CLI would resolve
        // to by default (`~/.mando`).
        MANDO_DATA_DIR: getDataDir(),
      },
    });
    return ok(stdout);
  } catch (e) {
    const stderr = (e as { stderr?: string }).stderr ?? '';
    return err(ioError(`${cmd} ${args.join(' ')}`, stderr || e));
  }
}

export async function codexAppUse(label: string): Promise<Result<void, ApiError>> {
  const result = await runMandoCli(['codex', 'app-use', label]);
  return result.map(() => undefined);
}

export async function codexAppRestore(): Promise<Result<void, ApiError>> {
  const result = await runMandoCli(['codex', 'app-restore']);
  return result.map(() => undefined);
}

export async function codexAppStatus(): Promise<
  Result<z.infer<typeof codexAppStatusSchema>, ApiError>
> {
  const result = await runMandoCli(['codex', 'app-status', '--json']);
  return result.andThen((stdout) =>
    parseJsonTextWith(stdout, codexAppStatusSchema, 'command:mando-codex-app-status'),
  );
}
