import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  applyPlanningProviderChange,
  displayedProvider,
  loadedDefaultProvider,
  normalizePremiumProvider,
  providerSelectionReady,
  submittedProvider,
  TASK_PROVIDER_DEFAULT,
  TASK_PROVIDER_PLANNING,
} from '../runtime/useInlineTaskCreate.helpers.ts';

describe('useInlineTaskCreate provider defaults', () => {
  it('defaults new normal tasks to Codex', () => {
    assert.equal(TASK_PROVIDER_DEFAULT, 'codex');
  });

  it('uses Claude Code while planning mode is enabled', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: true,
      wasPlanning: false,
      selectedProvider: null,
      prePlanningSelectedProvider: null,
    });

    assert.equal(result.selectedProvider, TASK_PROVIDER_PLANNING);
    assert.equal(result.prePlanningSelectedProvider, null);
  });

  it('restores Codex after leaving planning mode when Codex was selected before planning', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: false,
      wasPlanning: true,
      selectedProvider: TASK_PROVIDER_PLANNING,
      prePlanningSelectedProvider: null,
    });

    assert.equal(
      displayedProvider(result.selectedProvider, TASK_PROVIDER_DEFAULT),
      TASK_PROVIDER_DEFAULT,
    );
  });

  it('restores an explicit Claude choice after leaving planning mode', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: false,
      wasPlanning: true,
      selectedProvider: TASK_PROVIDER_PLANNING,
      prePlanningSelectedProvider: TASK_PROVIDER_PLANNING,
    });

    assert.equal(result.selectedProvider, TASK_PROVIDER_PLANNING);
  });

  it('does not overwrite the pre-planning provider on repeated planning-on calls', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: true,
      wasPlanning: true,
      selectedProvider: TASK_PROVIDER_PLANNING,
      prePlanningSelectedProvider: null,
    });

    assert.equal(result.selectedProvider, TASK_PROVIDER_PLANNING);
    assert.equal(result.prePlanningSelectedProvider, null);
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

    assert.equal(displayedProvider(null, defaultProvider), TASK_PROVIDER_PLANNING);
    assert.equal(submittedProvider(null, defaultProvider), TASK_PROVIDER_PLANNING);
  });

  it('submits an explicit provider before config loads', () => {
    const defaultProvider = loadedDefaultProvider(undefined, false);

    assert.equal(submittedProvider(TASK_PROVIDER_PLANNING, defaultProvider), 'claude');
  });

  it('waits for config before submit unless the user selected a provider', () => {
    assert.equal(providerSelectionReady(null, false), false);
    assert.equal(providerSelectionReady(TASK_PROVIDER_PLANNING, false), true);
    assert.equal(providerSelectionReady(null, true), true);
  });
});
