/**
 * IPC handlers for onboarding validation: Claude Code, Telegram, Linear.
 *
 * All validations hit real APIs even in sandbox mode — they are read-only
 * checks (Telegram getMe, Linear GraphQL query, `which claude`) with no
 * side effects. Sandbox isolation only applies to message sending, not
 * token/key validation.
 */
import { execFile } from 'child_process';
import { promisify } from 'util';
import { handleTrusted } from '#main/ipc-security';
import log from '#main/logger';

const execFileAsync = promisify(execFile);

/** Run a command and return stdout, or null on failure. */
async function run(
  cmd: string,
  args: string[],
  timeoutMs: number,
): Promise<{ stdout: string; stderr: string } | null> {
  try {
    return await execFileAsync(cmd, args, { encoding: 'utf-8', timeout: timeoutMs });
  } catch (e) {
    const stderr = (e as { stderr?: string }).stderr ?? '';
    log.warn(`[setup-validation] ${cmd} ${args.join(' ')} failed: ${stderr || e}`);
    return null;
  }
}

export function registerSetupValidationHandlers(): void {
  handleTrusted(
    'check-claude-code',
    async (): Promise<{ installed: boolean; version: string | null; works: boolean }> => {
      const which = await run('which', ['claude'], 5_000);
      if (!which) return { installed: false, version: null, works: false };

      let version: string | null = null;
      const ver = await run('claude', ['--version'], 10_000);
      if (ver) version = ver.stdout.trim();

      // Version check is sufficient — Mando provides its own API key to CC
      // workers via config, so CC doesn't need to be independently authenticated.
      return { installed: true, version, works: !!version };
    },
  );

  handleTrusted('validate-telegram-token', async (_e, token: string) => {
    try {
      const resp = await fetch(`https://api.telegram.org/bot${token}/getMe`);
      if (!resp.ok) {
        log.warn(`[setup-validation] Telegram getMe returned ${resp.status}`);
        return { valid: false, error: `Telegram API returned ${resp.status}` };
      }
      const data = (await resp.json()) as {
        ok: boolean;
        result?: { first_name: string; username: string };
      };
      if (data.ok && data.result) {
        return { valid: true, botName: data.result.first_name, botUsername: data.result.username };
      }
      return { valid: false, error: 'Invalid token' };
    } catch (e) {
      log.warn(`[setup-validation] Telegram validation failed: ${e}`);
      return { valid: false, error: e instanceof Error ? e.message : 'Network error' };
    }
  });

  handleTrusted('validate-linear-key', async (_e, apiKey: string) => {
    try {
      const resp = await fetch('https://api.linear.app/graphql', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: apiKey },
        body: JSON.stringify({ query: '{ teams { nodes { id key name } } }' }),
      });
      if (!resp.ok) {
        log.warn(`[setup-validation] Linear API returned ${resp.status}`);
        return { valid: false, teams: [], error: `Linear API returned ${resp.status}` };
      }
      const data = (await resp.json()) as {
        data?: { teams: { nodes: Array<{ id: string; key: string; name: string }> } };
        errors?: Array<{ message: string }>;
      };
      if (data.data?.teams?.nodes) {
        return { valid: true, teams: data.data.teams.nodes };
      }
      const msg = data.errors?.[0]?.message ?? 'Invalid API key';
      log.warn(`[setup-validation] Linear validation error: ${msg}`);
      return { valid: false, teams: [], error: msg };
    } catch (e) {
      log.warn(`[setup-validation] Linear validation failed: ${e}`);
      return { valid: false, teams: [], error: e instanceof Error ? e.message : 'Network error' };
    }
  });
}
