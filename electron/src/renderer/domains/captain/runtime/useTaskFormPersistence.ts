import { useCallback, useState } from 'react';
import { defineKeyspace, defineSlot } from '#renderer/global/providers/persistence';

const lastProjectSlot = defineSlot(
  'mando:lastProject',
  'domains/captain/runtime/useTaskFormPersistence',
);

const formStore = defineKeyspace('', 'domains/captain/runtime/useTaskFormPersistence');

/**
 * Encapsulates draft persistence for task creation forms.
 * Handles project selection memory and optional bulk-mode flag.
 */
export function useTaskFormPersistence(opts: {
  draftProjectKey: string;
  draftBulkKey?: string;
  hasDraft: boolean;
  initialProject?: string | null;
}) {
  const { draftProjectKey, draftBulkKey, hasDraft, initialProject } = opts;

  const [bulk, setBulkState] = useState(() =>
    draftBulkKey ? hasDraft && formStore.for(draftBulkKey).read() === '1' : false,
  );

  const [project, setProjectState] = useState(() => {
    if (hasDraft) {
      const saved = formStore.for(draftProjectKey).read();
      if (saved !== undefined) return saved;
    }
    return initialProject ?? lastProjectSlot.read() ?? '';
  });

  const setBulk = useCallback(
    (next: boolean) => {
      setBulkState(next);
      if (draftBulkKey) {
        if (next) formStore.for(draftBulkKey).write('1');
        else formStore.for(draftBulkKey).clear();
      }
    },
    [draftBulkKey],
  );

  const setProject = useCallback(
    (value: string) => {
      const resolved = value === '__all__' ? '' : value;
      setProjectState(resolved);
      if (resolved) {
        lastProjectSlot.write(value);
        formStore.for(draftProjectKey).write(value);
      } else {
        lastProjectSlot.clear();
        formStore.for(draftProjectKey).clear();
      }
    },
    [draftProjectKey],
  );

  const resetDrafts = useCallback(() => {
    setBulkState(false);
    if (draftBulkKey) formStore.for(draftBulkKey).clear();
    formStore.for(draftProjectKey).clear();
  }, [draftBulkKey, draftProjectKey]);

  const cleanupIfEmpty = useCallback(
    (titleEmpty: boolean) => {
      if (titleEmpty) {
        if (draftBulkKey) formStore.for(draftBulkKey).clear();
        formStore.for(draftProjectKey).clear();
      }
    },
    [draftBulkKey, draftProjectKey],
  );

  const persistProject = useCallback((proj: string) => {
    if (proj) lastProjectSlot.write(proj);
  }, []);

  return { bulk, setBulk, project, setProject, resetDrafts, cleanupIfEmpty, persistProject };
}
