import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  apiDeleteRouteR,
  apiPatchRouteR,
  apiPostRouteR,
  apiPutRouteR,
} from '#renderer/global/providers/http';
import { toWireConfig } from '#renderer/global/config/wireConfig';
import { toReactQuery } from '#result';
import type { MandoConfig as RendererMandoConfig } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';

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
