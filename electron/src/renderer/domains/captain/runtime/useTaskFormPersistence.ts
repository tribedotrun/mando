import { useCallback, useState } from 'react';
import { defineKeyspace, defineSlot } from '#renderer/global/providers/persistence';

const lastProjectSlot = defineSlot(
  'mando:lastProject',
  'domains/captain/runtime/useTaskFormPersistence',
);

const formProjectStore = defineKeyspace('', 'domains/captain/runtime/useTaskFormPersistence');

/**
 * Encapsulates draft persistence for task creation forms:
 * remembers the selected project across sessions.
 */
export function useTaskFormPersistence(opts: {
  draftProjectKey: string;
  hasDraft: boolean;
  initialProject?: string | null;
}) {
  const { draftProjectKey, hasDraft, initialProject } = opts;

  const [project, setProjectState] = useState(() => {
    if (hasDraft) {
      const saved = formProjectStore.for(draftProjectKey).read();
      if (saved !== undefined) return saved;
    }
    return initialProject ?? lastProjectSlot.read() ?? '';
  });

  const setProject = useCallback(
    (value: string) => {
      const resolved = value === '__all__' ? '' : value;
      setProjectState(resolved);
      if (resolved) {
        lastProjectSlot.write(value);
        formProjectStore.for(draftProjectKey).write(value);
      } else {
        lastProjectSlot.clear();
        formProjectStore.for(draftProjectKey).clear();
      }
    },
    [draftProjectKey],
  );

  const resetDrafts = useCallback(() => {
    formProjectStore.for(draftProjectKey).clear();
  }, [draftProjectKey]);

  const cleanupIfEmpty = useCallback(
    (titleEmpty: boolean) => {
      if (titleEmpty) formProjectStore.for(draftProjectKey).clear();
    },
    [draftProjectKey],
  );

  const persistProject = useCallback((proj: string) => {
    if (proj) lastProjectSlot.write(proj);
  }, []);

  return { project, setProject, resetDrafts, cleanupIfEmpty, persistProject };
}
