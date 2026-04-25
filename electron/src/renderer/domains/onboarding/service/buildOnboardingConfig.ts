import type { MandoConfig as RendererMandoConfig } from '#renderer/global/types';
import {
  applyOnboardingConfig,
  type OnboardingConfigOpts,
  toWireConfig,
} from '#renderer/global/config/wireConfig';
import type { MandoConfig } from '#shared/daemon-contract';

/**
 * Builds a MandoConfig from onboarding user inputs.
 * Used both for intermediate progress saves and final setup-complete.
 */
export function buildOnboardingConfig(
  opts: OnboardingConfigOpts,
  baseConfig?: RendererMandoConfig | null,
): MandoConfig {
  return toWireConfig(applyOnboardingConfig(baseConfig, opts));
}
