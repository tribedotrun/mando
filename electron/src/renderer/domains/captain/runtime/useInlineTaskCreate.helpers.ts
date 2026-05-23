import type { TaskProvider } from '#renderer/global/types';

export const TASK_PROVIDER_DEFAULT: TaskProvider = 'codex';
export const TASK_PROVIDER_PLANNING: TaskProvider = 'claude';

interface PlanningProviderChange {
  nextPlanning: boolean;
  wasPlanning: boolean;
  provider: TaskProvider;
  prePlanningProvider: TaskProvider;
}

interface PlanningProviderResult {
  provider: TaskProvider;
  prePlanningProvider: TaskProvider;
}

export function applyPlanningProviderChange({
  nextPlanning,
  wasPlanning,
  provider,
  prePlanningProvider,
}: PlanningProviderChange): PlanningProviderResult {
  if (nextPlanning === wasPlanning) {
    return { provider, prePlanningProvider };
  }

  if (nextPlanning) {
    return {
      provider: TASK_PROVIDER_PLANNING,
      prePlanningProvider: provider,
    };
  }

  return {
    provider: prePlanningProvider,
    prePlanningProvider,
  };
}
