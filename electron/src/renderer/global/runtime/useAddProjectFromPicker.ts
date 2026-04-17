import { useCallback, useState } from 'react';
import { useProjectAdd } from '#renderer/global/repo/configMutations';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/global/service/utils';
import log from '#renderer/global/service/logger';

/**
 * Shared hook for the "pick directory then add project" workflow.
 * Used by both SetupStepContent and TaskEmptyState.
 */
export function useAddProjectFromPicker() {
  const addProjectMut = useProjectAdd();
  const { selectDirectory } = useNativeActions();
  const [adding, setAdding] = useState(false);

  const pickAndAdd = useCallback(async () => {
    if (addProjectMut.isPending || adding) return;
    let dir: string | null;
    try {
      dir = await selectDirectory();
    } catch (err) {
      log.warn('[useAddProjectFromPicker] selectDirectory failed', err);
      toast.error(getErrorMessage(err, 'Failed to open folder picker'));
      return;
    }
    if (!dir) return;
    setAdding(true);
    try {
      await addProjectMut.mutateAsync({ path: dir });
    } catch (err) {
      log.warn('[useAddProjectFromPicker] addProject failed', err);
      toast.error(getErrorMessage(err, 'Failed to add project'));
    } finally {
      setAdding(false);
    }
  }, [addProjectMut, adding, selectDirectory]);

  return { pickAndAdd, adding: adding || addProjectMut.isPending };
}
