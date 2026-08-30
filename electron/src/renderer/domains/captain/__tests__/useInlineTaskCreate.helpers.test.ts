import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  displayedProvider,
  loadedDefaultProvider,
  normalizePremiumProvider,
  providerSelectionReady,
  submittedProvider,
  TASK_PROVIDER_DEFAULT,
} from '../runtime/useInlineTaskCreate.helpers.ts';

describe('useInlineTaskCreate provider defaults', () => {
  it('defaults new normal tasks to Codex', () => {
    assert.equal(TASK_PROVIDER_DEFAULT, 'codex');
  });

  it('normalizes OpenCode back to the premium default', () => {
    assert.equal(normalizePremiumProvider('opencode'), TASK_PROVIDER_DEFAULT);
  });

  it('does not synthesize a submit provider before config loads', () => {
    const defaultProvider = loadedDefaultProvider(undefined, false);

    assert.equal(displayedProvider(null, defaultProvider), TASK_PROVIDER_DEFAULT);
    assert.equal(submittedProvider(null, defaultProvider), undefined);
  });

  it('submits the loaded config default when the user has not selected a provider', () => {
    const defaultProvider = loadedDefaultProvider('claude', true);

    assert.equal(displayedProvider(null, defaultProvider), 'claude');
    assert.equal(submittedProvider(null, defaultProvider), 'claude');
  });

  it('submits an explicit provider before config loads', () => {
    const defaultProvider = loadedDefaultProvider(undefined, false);

    assert.equal(submittedProvider('claude', defaultProvider), 'claude');
  });

  it('waits for config before submit unless the user selected a provider', () => {
    assert.equal(providerSelectionReady(null, false), false);
    assert.equal(providerSelectionReady('claude', false), true);
    assert.equal(providerSelectionReady(null, true), true);
  });
});
