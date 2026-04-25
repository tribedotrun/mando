import { useState } from 'react';
import {
  useConfig,
  useProjectEdit,
  useProjectRemove,
} from '#renderer/domains/settings/runtime/hooks';
import { toast } from '#renderer/global/runtime/useFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import type { ProjectConfig } from '#renderer/global/types';

const EMPTY_PROJECTS: Record<string, ProjectConfig> = Object.freeze({});

export function useSettingsProjects() {
  const { data: config } = useConfig();
  const editMut = useProjectEdit();
  const removeMut = useProjectRemove();
  const projects = config?.captain?.projects ?? EMPTY_PROJECTS;

  const [editing, setEditing] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);

  const handleSave = (pathKey: string, project: ProjectConfig, updated: ProjectConfig) => {
    const currentName = project.name ?? pathKey;
    editMut.mutate(
      {
        currentName,
        rename: updated.name !== currentName ? updated.name : undefined,
        github_repo: updated.githubRepo || undefined,
        clear_github_repo: !updated.githubRepo && !!project.githubRepo ? true : undefined,
        aliases: updated.aliases ?? [],
        hooks: updated.hooks ?? {},
        preamble: updated.workerPreamble ?? '',
        check_command: updated.checkCommand ?? '',
        scout_summary: updated.scoutSummary ?? '',
      },
      {
        onSuccess: () => setEditing(null),
        onError: (err) => toast.error(getErrorMessage(err, 'Failed to save project')),
      },
    );
  };

  const handleRemove = (pathKey: string) => {
    removeMut.mutate(
      { name: projects[pathKey].name ?? pathKey },
      {
        onError: (err) => toast.error(getErrorMessage(err, 'Failed to delete project')),
        onSettled: () => setRemoving(null),
      },
    );
  };

  return {
    projects: { items: projects },
    editing: { value: editing, set: setEditing },
    removing: { value: removing, set: setRemoving },
    mutations: { editMut, removeMut },
    actions: { handleSave, handleRemove },
  };
}
