import type { MandoConfig } from '#renderer/global/types';

interface OnboardingConfigOpts {
  tgToken?: string;
  autoSchedule?: boolean;
}

/**
 * Builds a MandoConfig from onboarding user inputs.
 * Used both for intermediate progress saves and final setup-complete.
 */
export function buildOnboardingConfig(opts: OnboardingConfigOpts): MandoConfig {
  const config: MandoConfig = { features: { claudeCodeVerified: true } };
  if (opts.autoSchedule) {
    config.captain = { autoSchedule: true };
  }
  const env: Record<string, string> = {};
  if (opts.tgToken?.trim()) {
    config.channels = { telegram: { enabled: true } };
    env.TELEGRAM_MANDO_BOT_TOKEN = opts.tgToken.trim();
  }
  if (Object.keys(env).length > 0) config.env = env;
  return config;
}
