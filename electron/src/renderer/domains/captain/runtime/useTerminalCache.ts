import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { TerminalSessionInfo } from '#renderer/domains/captain/repo/queries';

/** Provides imperative access to the terminal sessions cache for TerminalPage. */
export function useTerminalCache() {
  const qc = useQueryClient();

  const getTerminals = useCallback(
    () => qc.getQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list()) ?? [],
    [qc],
  );

  const setTerminals = useCallback(
    (updater: (old: TerminalSessionInfo[] | undefined) => TerminalSessionInfo[]) => {
      qc.setQueryData<TerminalSessionInfo[]>(queryKeys.terminals.list(), updater);
    },
    [qc],
  );

  return { getTerminals, setTerminals };
}
