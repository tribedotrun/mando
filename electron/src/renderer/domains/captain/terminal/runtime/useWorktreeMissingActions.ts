import { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';

/// Re-probe the workbench's worktree by invalidating the workbenches
/// query. The refetch flows through the daemon's wire conversion, which
/// re-runs the filesystem probe against the live worktree path. Recovers
/// in place when the user recreates the worktree externally (e.g.
/// `git worktree add`).
export function useWorktreeMissingActions() {
  const qc = useQueryClient();
  const [retrying, setRetrying] = useState(false);

  const refresh = async () => {
    setRetrying(true);
    try {
      await qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
    } finally {
      setRetrying(false);
    }
  };

  return { refresh, retrying };
}
