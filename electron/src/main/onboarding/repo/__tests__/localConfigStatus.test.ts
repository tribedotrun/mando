import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { hasParsableLocalConfig } from '#main/onboarding/repo/localConfigStatus.ts';

describe('hasParsableLocalConfig', () => {
  it('accepts persisted config JSON that omits daemon API hydrated fields', () => {
    const persistedConfig = JSON.stringify({
      workspace: '~/.mando/workspace',
      captain: {
        autoSchedule: true,
      },
    });

    assert.equal(hasParsableLocalConfig(persistedConfig), true);
  });

  it('rejects malformed local config JSON', () => {
    assert.equal(hasParsableLocalConfig('{broken'), false);
  });

  it('rejects non-object JSON values', () => {
    assert.equal(hasParsableLocalConfig('[]'), false);
    assert.equal(hasParsableLocalConfig('null'), false);
  });
});
