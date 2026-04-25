import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  applyOnboardingConfig,
  DEFAULT_CLAUDE_TERMINAL_ARGS,
  DEFAULT_CODEX_TERMINAL_ARGS,
  DEFAULT_DASHBOARD_HOST,
  DEFAULT_DASHBOARD_PORT,
  DEFAULT_TICK_INTERVAL_S,
  DEFAULT_WORKSPACE,
  toWireConfig,
} from '../wireConfig.ts';

describe('toWireConfig', () => {
  it('fills the Rust config defaults for omitted renderer fields', () => {
    const config = toWireConfig({});

    assert.equal(config.workspace, DEFAULT_WORKSPACE);
    assert.equal(config.ui.openAtLogin, false);
    assert.deepEqual(config.features, {
      scout: false,
      setupDismissed: false,
      claudeCodeVerified: false,
    });
    assert.deepEqual(config.channels.telegram, { enabled: false, owner: '' });
    assert.deepEqual(config.gateway.dashboard, {
      host: DEFAULT_DASHBOARD_HOST,
      port: DEFAULT_DASHBOARD_PORT,
    });
    assert.equal(config.captain.autoSchedule, false);
    assert.equal(config.captain.autoMerge, false);
    assert.equal(config.captain.maxConcurrentWorkers, null);
    assert.equal(config.captain.tickIntervalS, DEFAULT_TICK_INTERVAL_S);
    assert.equal(config.captain.defaultTerminalAgent, 'claude');
    assert.equal(config.captain.claudeTerminalArgs, DEFAULT_CLAUDE_TERMINAL_ARGS);
    assert.equal(config.captain.codexTerminalArgs, DEFAULT_CODEX_TERMINAL_ARGS);
    assert.ok(config.captain.tz);
    assert.deepEqual(config.scout, {
      interests: { high: [], low: [] },
      userContext: { role: '', knownDomains: [], explainDomains: [] },
    });
    assert.deepEqual(config.env, {});
  });

  it('preserves environment-specific defaults when onboarding adds partial fields', () => {
    const config = toWireConfig(
      applyOnboardingConfig(
        {
          workspace: '/tmp/mando-sandbox/workspace',
          gateway: { dashboard: { host: '127.0.0.1', port: 18700 } },
          channels: { telegram: { enabled: true, owner: '999999' } },
          captain: { tickIntervalS: 30, autoSchedule: false },
          env: {
            TG_API_BASE_URL: 'http://127.0.0.1:19000',
          },
        },
        { tgToken: 'sandbox-token', autoSchedule: true },
      ),
    );

    assert.equal(config.workspace, '/tmp/mando-sandbox/workspace');
    assert.deepEqual(config.gateway.dashboard, { host: '127.0.0.1', port: 18700 });
    assert.equal(config.channels.telegram.owner, '999999');
    assert.equal(config.channels.telegram.enabled, true);
    assert.equal(config.captain.autoSchedule, true);
    assert.equal(config.captain.tickIntervalS, 30);
    assert.deepEqual(config.env, {
      TG_API_BASE_URL: 'http://127.0.0.1:19000',
      TELEGRAM_MANDO_BOT_TOKEN: 'sandbox-token',
    });
  });
});
