import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  applyPlanningProviderChange,
  TASK_PROVIDER_DEFAULT,
  TASK_PROVIDER_PLANNING,
} from '../runtime/useInlineTaskCreate.helpers.ts';

describe('useInlineTaskCreate provider defaults', () => {
  it('defaults new normal tasks to Codex', () => {
    assert.equal(TASK_PROVIDER_DEFAULT, 'codex');
  });

  it('forces Claude while planning mode is enabled', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: true,
      wasPlanning: false,
      provider: TASK_PROVIDER_DEFAULT,
      prePlanningProvider: TASK_PROVIDER_DEFAULT,
    });

    assert.equal(result.provider, TASK_PROVIDER_PLANNING);
    assert.equal(result.prePlanningProvider, TASK_PROVIDER_DEFAULT);
  });

  it('restores Codex after leaving planning mode when Codex was selected before planning', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: false,
      wasPlanning: true,
      provider: TASK_PROVIDER_PLANNING,
      prePlanningProvider: TASK_PROVIDER_DEFAULT,
    });

    assert.equal(result.provider, TASK_PROVIDER_DEFAULT);
  });

  it('restores an explicit Claude choice after leaving planning mode', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: false,
      wasPlanning: true,
      provider: TASK_PROVIDER_PLANNING,
      prePlanningProvider: TASK_PROVIDER_PLANNING,
    });

    assert.equal(result.provider, TASK_PROVIDER_PLANNING);
  });

  it('does not overwrite the pre-planning provider on repeated planning-on calls', () => {
    const result = applyPlanningProviderChange({
      nextPlanning: true,
      wasPlanning: true,
      provider: TASK_PROVIDER_PLANNING,
      prePlanningProvider: TASK_PROVIDER_DEFAULT,
    });

    assert.equal(result.provider, TASK_PROVIDER_PLANNING);
    assert.equal(result.prePlanningProvider, TASK_PROVIDER_DEFAULT);
  });
});
