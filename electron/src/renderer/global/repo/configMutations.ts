import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  apiDeleteRouteR,
  apiPatchRouteR,
  apiPostRouteR,
  apiPutRouteR,
} from '#renderer/global/providers/http';
import { toReactQuery } from '#result';
import type {
  MandoConfig as RendererMandoConfig,
  ProjectConfig as RendererProjectConfig,
} from '#renderer/global/types';
import type {
  MandoConfig as WireMandoConfig,
  ProjectConfig as WireProjectConfig,
} from '#shared/daemon-contract';
import { queryKeys } from '#renderer/global/repo/queryKeys';

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

function toWireConfig(config: RendererMandoConfig): WireMandoConfig {
  return {
    workspace: config.workspace ?? '',
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
        host: config.gateway?.dashboard?.host ?? '127.0.0.1',
        port: config.gateway?.dashboard?.port ?? 18791,
      },
    },
    captain: {
      autoSchedule: config.captain?.autoSchedule ?? false,
      autoMerge: config.captain?.autoMerge ?? false,
      maxConcurrentWorkers: config.captain?.maxConcurrentWorkers ?? null,
      tickIntervalS: config.captain?.tickIntervalS ?? 30,
      tz: config.captain?.tz ?? 'UTC',
      defaultTerminalAgent: config.captain?.defaultTerminalAgent ?? 'claude',
      claudeTerminalArgs: config.captain?.claudeTerminalArgs ?? '',
      codexTerminalArgs: config.captain?.codexTerminalArgs ?? '',
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

// ---------------------------------------------------------------------------
// useConfigSave
// ---------------------------------------------------------------------------

export function useConfigSave() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (config: RendererMandoConfig) =>
      toReactQuery(apiPutRouteR('putConfig', toWireConfig(config))),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useProjectAdd
// ---------------------------------------------------------------------------

export function useProjectAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { path: string }) =>
      toReactQuery(
        apiPostRouteR('postProjects', {
          name: undefined,
          path: vars.path,
          aliases: [],
        }),
      ),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useProjectEdit
// ---------------------------------------------------------------------------

export interface ProjectEditInput {
  /** Current project name (used in the URL path). */
  currentName: string;
  rename?: string;
  github_repo?: string;
  clear_github_repo?: boolean;
  aliases?: string[];
  hooks?: Record<string, string>;
  preamble?: string;
  check_command?: string;
  scout_summary?: string;
}

export function useProjectEdit() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: ProjectEditInput) => {
      const { currentName, ...body } = vars;
      return toReactQuery(
        apiPatchRouteR('patchProjectsByName', body, {
          params: { name: currentName },
        }),
      );
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}

// ---------------------------------------------------------------------------
// useProjectRemove
// ---------------------------------------------------------------------------

export function useProjectRemove() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { name: string }) =>
      toReactQuery(
        apiDeleteRouteR('deleteProjectsByName', {
          params: { name: vars.name },
        }),
      ),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}
