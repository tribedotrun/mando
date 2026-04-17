import { useCallback, useState } from 'react';

const LAST_PROJECT_KEY = 'mando:lastProject';

/**
 * Encapsulates localStorage-based draft persistence for task creation forms.
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
    draftBulkKey ? hasDraft && localStorage.getItem(draftBulkKey) === '1' : false,
  );

  const [project, setProjectState] = useState(() => {
    if (hasDraft) {
      const saved = localStorage.getItem(draftProjectKey);
      if (saved !== null) return saved;
    }
    return initialProject ?? localStorage.getItem(LAST_PROJECT_KEY) ?? '';
  });

  const setBulk = useCallback(
    (next: boolean) => {
      setBulkState(next);
      if (draftBulkKey) {
        if (next) localStorage.setItem(draftBulkKey, '1');
        else localStorage.removeItem(draftBulkKey);
      }
    },
    [draftBulkKey],
  );

  const setProject = useCallback(
    (value: string) => {
      const resolved = value === '__all__' ? '' : value;
      setProjectState(resolved);
      if (resolved) {
        localStorage.setItem(LAST_PROJECT_KEY, value);
        localStorage.setItem(draftProjectKey, value);
      } else {
        localStorage.removeItem(LAST_PROJECT_KEY);
        localStorage.removeItem(draftProjectKey);
      }
    },
    [draftProjectKey],
  );

  const resetDrafts = useCallback(() => {
    setBulkState(false);
    if (draftBulkKey) localStorage.removeItem(draftBulkKey);
    localStorage.removeItem(draftProjectKey);
  }, [draftBulkKey, draftProjectKey]);

  const cleanupIfEmpty = useCallback(
    (titleEmpty: boolean) => {
      if (titleEmpty) {
        if (draftBulkKey) localStorage.removeItem(draftBulkKey);
        localStorage.removeItem(draftProjectKey);
      }
    },
    [draftBulkKey, draftProjectKey],
  );

  const persistProject = useCallback((proj: string) => {
    if (proj) localStorage.setItem(LAST_PROJECT_KEY, proj);
  }, []);

  return { bulk, setBulk, project, setProject, resetDrafts, cleanupIfEmpty, persistProject };
}
