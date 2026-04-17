import { useCallback } from 'react';
import { toast } from 'sonner';
import {
  useProjectAdd,
  useProjectEdit,
  useProjectRemove,
} from '#renderer/global/repo/configMutations';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { getErrorMessage } from '#renderer/global/service/utils';

interface ProjectWorkflowDeps {
  navigate: (opts: { to: string; search?: Record<string, string> }) => void;
  projectFilter: string | null;
}

/**
 * Encapsulates project add/rename/remove workflows including
 * navigation side-effects and toast feedback.
 */
export function useProjectWorkflows({ navigate, projectFilter }: ProjectWorkflowDeps) {
  const addProjectMut = useProjectAdd();
  const editProjectMut = useProjectEdit();
  const removeProjectMut = useProjectRemove();
  const { selectDirectory } = useNativeActions();

  const addProject = useCallback(() => {
    void (async () => {
      try {
        const dir = await selectDirectory();
        if (!dir) return;
        addProjectMut.mutate({ path: dir });
      } catch (err) {
        toast.error(getErrorMessage(err, 'Failed to add project'));
      }
    })();
  }, [selectDirectory, addProjectMut]);

  const renameProject = useCallback(
    async (oldName: string, newName: string) => {
      try {
        await editProjectMut.mutateAsync({ currentName: oldName, rename: newName });
        if (projectFilter === oldName) {
          void navigate({ to: '/', search: { project: newName } });
        }
        toast.success(`Renamed to "${newName}"`);
      } catch (err) {
        toast.error(getErrorMessage(err, 'Failed to rename project'));
      }
    },
    [editProjectMut, projectFilter, navigate],
  );

  const removeProject = useCallback(
    async (name: string) => {
      const res = await removeProjectMut.mutateAsync({ name });
      if (projectFilter === name) {
        void navigate({ to: '/', search: {} });
      }
      const taskMsg =
        res.deleted_tasks > 0
          ? ` and ${res.deleted_tasks} task${res.deleted_tasks !== 1 ? 's' : ''}`
          : '';
      toast.success(`Deleted "${name}"${taskMsg}`);
    },
    [removeProjectMut, projectFilter, navigate],
  );

  return { addProject, renameProject, removeProject };
}
