import type { TaskProvider } from '#renderer/global/types';

export type PremiumTaskProvider = Extract<TaskProvider, 'claude' | 'codex'>;

export const TASK_PROVIDER_DEFAULT: PremiumTaskProvider = 'codex';
export const TASK_PROVIDER_PLANNING: PremiumTaskProvider = 'claude';

interface PlanningProviderChange {
  nextPlanning: boolean;
  wasPlanning: boolean;
  selectedProvider: PremiumTaskProvider | null;
  prePlanningSelectedProvider: PremiumTaskProvider | null;
}

interface PlanningProviderResult {
  selectedProvider: PremiumTaskProvider | null;
  prePlanningSelectedProvider: PremiumTaskProvider | null;
}

export function normalizePremiumProvider(
  provider: TaskProvider | null | undefined,
): PremiumTaskProvider {
  return provider === 'claude' ? 'claude' : TASK_PROVIDER_DEFAULT;
}

export function loadedDefaultProvider(
  provider: TaskProvider | null | undefined,
  configLoaded: boolean,
): PremiumTaskProvider | null {
  return configLoaded ? normalizePremiumProvider(provider) : null;
}

export function displayedProvider(
  selectedProvider: PremiumTaskProvider | null,
  defaultProvider: PremiumTaskProvider | null,
): PremiumTaskProvider {
  return selectedProvider ?? defaultProvider ?? TASK_PROVIDER_DEFAULT;
}

export function submittedProvider(
  selectedProvider: PremiumTaskProvider | null,
  defaultProvider: PremiumTaskProvider | null,
): PremiumTaskProvider | undefined {
  return selectedProvider ?? defaultProvider ?? undefined;
}

export function providerSelectionReady(
  selectedProvider: PremiumTaskProvider | null,
  configLoaded: boolean,
): boolean {
  return selectedProvider !== null || configLoaded;
}

export function applyPlanningProviderChange({
  nextPlanning,
  wasPlanning,
  selectedProvider,
  prePlanningSelectedProvider,
}: PlanningProviderChange): PlanningProviderResult {
  if (nextPlanning === wasPlanning) {
    return { selectedProvider, prePlanningSelectedProvider };
  }

  if (nextPlanning) {
    return {
      selectedProvider: TASK_PROVIDER_PLANNING,
      prePlanningSelectedProvider: selectedProvider,
    };
  }

  return {
    selectedProvider: prePlanningSelectedProvider,
    prePlanningSelectedProvider,
  };
}
