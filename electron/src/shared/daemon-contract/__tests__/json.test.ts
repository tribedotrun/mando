import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { parseConfigJsonText, parseUpgradedConfigJsonText } from '../json.ts';

/** A schema-valid `config.json`, matching what the Rust loader writes. */
function validConfig() {
  return {
    workspace: '~/.mando/workspace',
    ui: { openAtLogin: false },
    features: { scout: false, setupDismissed: false, claudeCodeVerified: false },
    channels: { telegram: { enabled: false, owner: '' } },
    gateway: { dashboard: { host: '127.0.0.1', port: 18791 } },
    captain: {
      autoSchedule: false,
      autoMerge: false,
      maxConcurrentWorkers: null,
      tickIntervalS: 30,
      tz: 'America/Mexico_City',
      defaultTaskAgent: 'codex',
      defaultGlmImplementation: false,
      projects: {},
    },
    scout: {
      interests: { high: [], low: [] },
      userContext: { role: '', knownDomains: [], explainDomains: [] },
    },
    env: {},
  };
}

/** The same file as written by a build from before the in-app terminal was
 *  removed: three keys the current `Config` struct no longer declares. */
function preUpgradeConfigJson(): string {
  const config = validConfig();
  return JSON.stringify({
    ...config,
    captain: {
      ...config.captain,
      defaultTerminalAgent: 'claude',
      claudeTerminalArgs: '--dangerously-skip-permissions',
      codexTerminalArgs: '--full-auto',
    },
  });
}

describe('config JSON read from disk', () => {
  it('rejects retired keys under the strict daemon schema', () => {
    // The daemon never sends these, so the wire parser is right to refuse
    // them — which is why the local-file fallback needs its own reader.
    assert.ok(parseConfigJsonText(preUpgradeConfigJson(), 'test').isErr());
  });

  it('drops keys retired by a newer build, like the Rust loader does', () => {
    const parsed = parseUpgradedConfigJsonText(preUpgradeConfigJson(), 'test');
    assert.ok(parsed.isOk(), 'a pre-upgrade config.json must still load');
    if (!parsed.isOk()) return;

    assert.equal(parsed.value.captain.tickIntervalS, 30);
    assert.equal(parsed.value.captain.defaultTaskAgent, 'codex');
    assert.deepEqual(Object.keys(parsed.value.captain).sort(), [
      'autoMerge',
      'autoSchedule',
      'defaultGlmImplementation',
      'defaultTaskAgent',
      'maxConcurrentWorkers',
      'projects',
      'tickIntervalS',
      'tz',
    ]);
  });

  it('drops an unknown key nested below the top level', () => {
    const config = validConfig();
    const raw = JSON.stringify({
      ...config,
      gateway: { dashboard: { ...config.gateway.dashboard, legacyPort: 18000 } },
    });

    const parsed = parseUpgradedConfigJsonText(raw, 'test');
    assert.ok(parsed.isOk());
    if (!parsed.isOk()) return;
    assert.equal(parsed.value.gateway.dashboard.port, 18791);
    assert.ok(!('legacyPort' in parsed.value.gateway.dashboard));
  });

  it('leaves an already-current config unchanged', () => {
    const parsed = parseUpgradedConfigJsonText(JSON.stringify(validConfig()), 'test');
    assert.ok(parsed.isOk());
    if (!parsed.isOk()) return;
    assert.deepEqual(parsed.value, validConfig());
  });

  it('still rejects a genuinely malformed config', () => {
    // Tolerance covers unknown keys only. A wrong type on a known key is
    // corruption and must surface.
    const config = validConfig();
    const raw = JSON.stringify({
      ...config,
      captain: { ...config.captain, tickIntervalS: 'soon' },
    });
    assert.ok(parseUpgradedConfigJsonText(raw, 'test').isErr());
  });

  it('still rejects text that is not JSON', () => {
    assert.ok(parseUpgradedConfigJsonText('not json at all', 'test').isErr());
  });
});
