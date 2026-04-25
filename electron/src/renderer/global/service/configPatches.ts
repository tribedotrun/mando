import type {
  MandoConfig,
  ScoutConfig,
  CaptainConfig,
  TelegramConfig,
  FeaturesConfig,
} from '#renderer/global/types';
import {
  DEFAULT_CLAUDE_TERMINAL_ARGS,
  DEFAULT_CODEX_TERMINAL_ARGS,
} from '#renderer/global/config/wireConfig';

export type ConfigTransform = (current: MandoConfig) => MandoConfig;

/** Default CLI arguments for worker agents. */
export const CLAUDE_ARGS_DEFAULT = DEFAULT_CLAUDE_TERMINAL_ARGS;
export const CODEX_ARGS_DEFAULT = DEFAULT_CODEX_TERMINAL_ARGS;

/** Patches the scout sub-tree. */
export function scoutPatch(patch: Partial<ScoutConfig>): ConfigTransform {
  return (c) => ({ ...c, scout: { ...(c.scout || {}), ...patch } });
}

/** Patches the env sub-tree. */
export function envPatch(patch: Record<string, string>): ConfigTransform {
  return (c) => ({ ...c, env: { ...(c.env || {}), ...patch } });
}

/** Patches the captain sub-tree. */
export function captainPatch(patch: Partial<CaptainConfig>): ConfigTransform {
  return (c) => ({ ...c, captain: { ...(c.captain || {}), ...patch } });
}

/** Patches the channels.telegram sub-tree. */
export function telegramPatch(patch: Partial<TelegramConfig>): ConfigTransform {
  return (c) => ({
    ...c,
    channels: {
      ...c.channels,
      telegram: { ...(c.channels?.telegram || {}), ...patch },
    },
  });
}

/** Patches the features sub-tree. */
export function featuresPatch(patch: Partial<FeaturesConfig>): ConfigTransform {
  return (c) => ({ ...c, features: { ...(c.features || {}), ...patch } });
}

/** Composes multiple config transforms into one (applied left to right). */
export function composePatch(...transforms: ConfigTransform[]): ConfigTransform {
  return (c) => transforms.reduce((cfg, fn) => fn(cfg), c);
}

/** Patches scout.interests deeply. */
export function scoutInterestsPatch(patch: Record<string, unknown>): ConfigTransform {
  return (c) => scoutPatch({ interests: { ...(c.scout?.interests ?? {}), ...patch } })(c);
}

/** Patches scout.userContext deeply. */
export function scoutUserContextPatch(patch: Record<string, unknown>): ConfigTransform {
  return (c) => scoutPatch({ userContext: { ...(c.scout?.userContext ?? {}), ...patch } })(c);
}
