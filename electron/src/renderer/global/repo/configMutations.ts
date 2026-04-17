import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { apiPost, apiPut, apiPatch, apiDel } from '#renderer/global/providers/http';
import type { MandoConfig } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';

// ---------------------------------------------------------------------------
// useConfigSave
// ---------------------------------------------------------------------------

export function useConfigSave() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (config: MandoConfig) => apiPut<{ ok: boolean }>('/api/config', config),
    onError: () => {
      toast.error('Failed to save config');
    },
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
    mutationFn: async (vars: { path: string }) =>
      apiPost<{ ok: boolean; name: string; path: string; githubRepo: string }>('/api/projects', {
        path: vars.path,
      }),
    onError: () => {
      toast.error('Failed to add project');
    },
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
    mutationFn: async (vars: ProjectEditInput) => {
      const { currentName, ...body } = vars;
      return apiPatch<{ ok: boolean }>(`/api/projects/${encodeURIComponent(currentName)}`, body);
    },
    onError: () => {
      toast.error('Failed to save project');
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
    mutationFn: async (vars: { name: string }) =>
      apiDel<{ ok: boolean; deleted_tasks: number }>(
        `/api/projects/${encodeURIComponent(vars.name)}`,
      ),
    onError: () => {
      toast.error('Failed to remove project');
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    },
  });
}
