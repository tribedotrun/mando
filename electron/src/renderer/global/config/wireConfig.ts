import type {
  MandoConfig as RendererMandoConfig,
  ProjectConfig as RendererProjectConfig,
} from '#renderer/global/types';
import type {
  MandoConfig as WireMandoConfig,
  ProjectConfig as WireProjectConfig,
} from '#shared/daemon-contract';

export const DEFAULT_WORKSPACE = '~/.mando/workspace';
export const DEFAULT_DASHBOARD_HOST = '127.0.0.1';
export const DEFAULT_DASHBOARD_PORT = 18791;
export const DEFAULT_TICK_INTERVAL_S = 30;
export const DEFAULT_TERMINAL_AGENT = 'claude';
export const DEFAULT_CLAUDE_TERMINAL_ARGS = '--dangerously-skip-permissions';
export const DEFAULT_CODEX_TERMINAL_ARGS = '--full-auto';

export interface OnboardingConfigOpts {
  tgToken?: string;
  autoSchedule?: boolean;
}

function defaultTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
}

function toWireProjectConfig(project: RendererProjectConfig | undefined): WireProjectConfig {
  return {
    name: project?.name ?? '',
    path: project?.path ?? '',
    githubRepo: project?.githubRepo ?? null,
    logo: project?.logo ?? null,
    aliases: project?.aliases ?? [],
    hooks: project?.hooks ?? {},
    workerPreamble: project?.workerPreamble ?? '',
    scoutSummary: project?.scoutSummary ?? '',
    checkCommand: project?.checkCommand ?? '',
    classifyRules: project?.classifyRules ?? [],
  };
}

function fromWireProjectConfig(project: WireProjectConfig | undefined): RendererProjectConfig {
  return {
    name: project?.name || undefined,
    path: project?.path || undefined,
    githubRepo: project?.githubRepo ?? null,
    logo: project?.logo ?? null,
    aliases: project?.aliases ?? [],
    hooks: project?.hooks ?? {},
    workerPreamble: project?.workerPreamble || undefined,
    scoutSummary: project?.scoutSummary || undefined,
    checkCommand: project?.checkCommand || undefined,
    classifyRules: project?.classifyRules ?? [],
  };
}

function fromWireTerminalAgent(value: string | null | undefined): 'claude' | 'codex' | undefined {
  return value === 'claude' || value === 'codex' ? value : undefined;
}

export function applyOnboardingConfig(
  baseConfig: RendererMandoConfig | null | undefined,
  opts: OnboardingConfigOpts,
): RendererMandoConfig {
  const config: RendererMandoConfig = {
    ...baseConfig,
    features: {
      ...baseConfig?.features,
      claudeCodeVerified: true,
    },
  };

  if (opts.autoSchedule) {
    config.captain = { ...baseConfig?.captain, autoSchedule: true };
  }

  const env: Record<string, string> = { ...(baseConfig?.env ?? {}) };
  if (opts.tgToken?.trim()) {
    config.channels = {
      ...baseConfig?.channels,
      telegram: {
        ...baseConfig?.channels?.telegram,
        enabled: true,
      },
    };
    env.TELEGRAM_MANDO_BOT_TOKEN = opts.tgToken.trim();
  }
  if (Object.keys(env).length > 0) config.env = env;

  return config;
}

export function toWireConfig(config: RendererMandoConfig): WireMandoConfig {
  return {
    workspace: config.workspace ?? DEFAULT_WORKSPACE,
    ui: {
      openAtLogin: config.ui?.openAtLogin ?? false,
    },
    features: {
      scout: config.features?.scout ?? false,
      setupDismissed: config.features?.setupDismissed ?? false,
      claudeCodeVerified: config.features?.claudeCodeVerified ?? false,
    },
    channels: {
      telegram: {
        enabled: config.channels?.telegram?.enabled ?? false,
        owner: config.channels?.telegram?.owner ?? '',
      },
    },
    gateway: {
      dashboard: {
        host: config.gateway?.dashboard?.host ?? DEFAULT_DASHBOARD_HOST,
        port: config.gateway?.dashboard?.port ?? DEFAULT_DASHBOARD_PORT,
      },
    },
    captain: {
      autoSchedule: config.captain?.autoSchedule ?? false,
      autoMerge: config.captain?.autoMerge ?? false,
      maxConcurrentWorkers: config.captain?.maxConcurrentWorkers ?? null,
      tickIntervalS: config.captain?.tickIntervalS ?? DEFAULT_TICK_INTERVAL_S,
      tz: config.captain?.tz ?? defaultTimeZone(),
      defaultTerminalAgent: config.captain?.defaultTerminalAgent ?? DEFAULT_TERMINAL_AGENT,
      claudeTerminalArgs: config.captain?.claudeTerminalArgs ?? DEFAULT_CLAUDE_TERMINAL_ARGS,
      codexTerminalArgs: config.captain?.codexTerminalArgs ?? DEFAULT_CODEX_TERMINAL_ARGS,
      projects: Object.fromEntries(
        Object.entries(config.captain?.projects ?? {}).map(([key, project]) => [
          key,
          toWireProjectConfig(project),
        ]),
      ),
    },
    scout: {
      interests: {
        high: config.scout?.interests?.high ?? [],
        low: config.scout?.interests?.low ?? [],
      },
      userContext: {
        role: config.scout?.userContext?.role ?? '',
        knownDomains: config.scout?.userContext?.knownDomains ?? [],
        explainDomains: config.scout?.userContext?.explainDomains ?? [],
      },
    },
    env: config.env ?? {},
  };
}

export function fromWireConfig(config: WireMandoConfig): RendererMandoConfig {
  return {
    workspace: config.workspace || undefined,
    ui: config.ui
      ? {
          openAtLogin: config.ui.openAtLogin ?? undefined,
        }
      : undefined,
    features: config.features
      ? {
          scout: config.features.scout ?? undefined,
          setupDismissed: config.features.setupDismissed ?? undefined,
          claudeCodeVerified: config.features.claudeCodeVerified ?? undefined,
        }
      : undefined,
    channels: config.channels
      ? {
          telegram: config.channels.telegram
            ? {
                enabled: config.channels.telegram.enabled ?? undefined,
                owner: config.channels.telegram.owner || undefined,
              }
            : undefined,
        }
      : undefined,
    gateway: config.gateway
      ? {
          dashboard: config.gateway.dashboard
            ? {
                host: config.gateway.dashboard.host || undefined,
                port: config.gateway.dashboard.port ?? undefined,
              }
            : undefined,
        }
      : undefined,
    captain: config.captain
      ? {
          autoSchedule: config.captain.autoSchedule ?? undefined,
          autoMerge: config.captain.autoMerge ?? undefined,
          maxConcurrentWorkers: config.captain.maxConcurrentWorkers ?? undefined,
          tickIntervalS: config.captain.tickIntervalS ?? undefined,
          tz: config.captain.tz || undefined,
          defaultTerminalAgent: fromWireTerminalAgent(config.captain.defaultTerminalAgent),
          claudeTerminalArgs: config.captain.claudeTerminalArgs || undefined,
          codexTerminalArgs: config.captain.codexTerminalArgs || undefined,
          projects: config.captain.projects
            ? Object.fromEntries(
                Object.entries(config.captain.projects).map(([key, project]) => [
                  key,
                  fromWireProjectConfig(project),
                ]),
              )
            : undefined,
        }
      : undefined,
    scout: config.scout
      ? {
          interests: config.scout.interests
            ? {
                high: config.scout.interests.high ?? [],
                low: config.scout.interests.low ?? [],
              }
            : undefined,
          userContext: config.scout.userContext
            ? {
                role: config.scout.userContext.role || undefined,
                knownDomains: config.scout.userContext.knownDomains ?? [],
                explainDomains: config.scout.userContext.explainDomains ?? [],
              }
            : undefined,
        }
      : undefined,
    env: config.env ?? {},
  };
}
