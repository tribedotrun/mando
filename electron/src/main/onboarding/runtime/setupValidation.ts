/**
 * IPC handlers for onboarding validation: Claude Code, Telegram.
 *
 * All validations hit real APIs even in sandbox mode — they are read-only
 * checks (Telegram getMe, `which claude`) with no side effects. Sandbox
 * isolation only applies to message sending, not token/key validation.
 */
import { execFile } from 'child_process';
import { promisify } from 'util';
import { z } from 'zod';
import { handleChannel } from '#main/global/runtime/ipcSecurity';
import { currentPath } from '#main/global/service/launchd';
import log from '#main/global/providers/logger';

// Telegram getMe response shape per https://core.telegram.org/bots/api#getme.
// Only the fields we actually consume are required; the rest is ignored.
const telegramGetMeSchema = z.object({
  ok: z.boolean(),
  result: z
    .object({
      first_name: z.string(),
      username: z.string(),
    })
    .optional(),
});

const execFileAsync = promisify(execFile);

/** Run a command with the full user PATH (packaged apps get a minimal PATH). */
// invariant: errors are caught and return null; null is the valid "command failed" signal to callers inside this module only
async function run(
  cmd: string,
  args: string[],
  timeoutMs: number,
): Promise<{ stdout: string; stderr: string } | null> {
  try {
    return await execFileAsync(cmd, args, {
      encoding: 'utf-8',
      timeout: timeoutMs,
      env: { ...process.env, PATH: currentPath() },
    });
  } catch (e) {
    const stderr = (e as { stderr?: string }).stderr ?? '';
    log.warn(`[setup-validation] ${cmd} ${args.join(' ')} failed: ${stderr || e}`);
    return null;
  }
}

export function registerSetupValidationHandlers(): void {
  handleChannel('check-claude-code', async () => {
    const which = await run('which', ['claude'], 5_000);
    if (!which) return { installed: false, version: null, works: false };

    let version: string | null = null;
    const ver = await run('claude', ['--version'], 10_000);
    if (ver) version = ver.stdout.trim();

    // Version check is sufficient — Mando provides its own API key to CC
    // workers via config, so CC doesn't need to be independently authenticated.
    return { installed: true, version, works: !!version };
  });

  handleChannel('validate-telegram-token', async (_event, token) => {
    try {
      const resp = await fetch(`https://api.telegram.org/bot${token}/getMe`);
      if (!resp.ok) {
        log.warn(`[setup-validation] Telegram getMe returned ${resp.status}`);
        return { valid: false, error: `Telegram API returned ${resp.status}` };
      }
      const rawJson: unknown = await resp.json();
      const parsed = telegramGetMeSchema.safeParse(rawJson);
      if (!parsed.success) {
        log.warn('[setup-validation] Telegram getMe response failed schema parse');
        return { valid: false, error: 'Telegram API response was malformed' };
      }
      const data = parsed.data;
      if (data.ok && data.result) {
        return { valid: true, botName: data.result.first_name, botUsername: data.result.username };
      }
      return { valid: false, error: 'Invalid token' };
    } catch (e) {
      log.warn(`[setup-validation] Telegram validation failed: ${e}`);
      return { valid: false, error: e instanceof Error ? e.message : 'Network error' };
    }
  });
}
