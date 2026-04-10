import { useCallback, useRef, useState } from 'react';
import { createWorktree } from '#renderer/api-terminal';
import { apiPost } from '#renderer/api';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/queryKeys';

export interface TerminalPageState {
  project: string;
  cwd: string;
  resumeSessionId?: string | null;
  name?: string | null;
  preparing?: boolean;
}

export function useWorktreeTerminal() {
  const qc = useQueryClient();
  const [terminalPage, setTerminalPage] = useState<TerminalPageState | null>(null);
  const wtGenRef = useRef(0);
  const creatingRef = useRef(false);

  const openNewTerminal = useCallback(async (project: string, onReady?: (cwd: string) => void) => {
    if (creatingRef.current) return;
    creatingRef.current = true;
    const gen = ++wtGenRef.current;
    setTerminalPage({ project, cwd: '', preparing: true });
    try {
      const now = new Date();
      const suffix = [
        String(now.getMonth() + 1).padStart(2, '0'),
        String(now.getDate()).padStart(2, '0'),
        '-',
        String(now.getHours()).padStart(2, '0'),
        String(now.getMinutes()).padStart(2, '0'),
        String(now.getSeconds()).padStart(2, '0'),
      ].join('');
      const result = await createWorktree(project, suffix);
      if (wtGenRef.current !== gen) {
        apiPost('/api/worktrees/remove', { path: result.path }).catch((e) => console.error(e));
        return;
      }
      setTerminalPage({ project, cwd: result.path });
      void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
      onReady?.(result.path);
    } catch (err) {
      if (wtGenRef.current !== gen) return;
      log.error('createWorktree failed', err);
      toast.error(getErrorMessage(err, 'Failed to create workspace'));
      setTerminalPage(null);
    } finally {
      creatingRef.current = false;
    }
  }, []);

  const cancelPreparing = useCallback(() => {
    ++wtGenRef.current;
    setTerminalPage(null);
  }, []);

  return { terminalPage, setTerminalPage, openNewTerminal, cancelPreparing };
}
