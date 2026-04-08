import { useCallback, useRef, useState } from 'react';
import { createWorktree } from '#renderer/api-terminal';
import { apiPost } from '#renderer/api';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';

export interface TerminalPageState {
  project: string;
  cwd: string;
  resumeSessionId?: string | null;
  preparing?: boolean;
}

export function useWorktreeTerminal() {
  const [terminalPage, setTerminalPage] = useState<TerminalPageState | null>(null);
  const wtGenRef = useRef(0);

  const openNewTerminal = useCallback(async (project: string) => {
    const gen = ++wtGenRef.current;
    setTerminalPage({ project, cwd: '', preparing: true });
    try {
      const suffix = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
      const result = await createWorktree(project, suffix);
      if (wtGenRef.current !== gen) {
        apiPost('/api/worktrees/remove', { path: result.path }).catch((e) => console.error(e));
        return;
      }
      setTerminalPage({ project, cwd: result.path });
    } catch (err) {
      if (wtGenRef.current !== gen) return;
      log.error('createWorktree failed', err);
      toast.error(getErrorMessage(err, 'Failed to create workspace'));
      setTerminalPage(null);
    }
  }, []);

  const cancelPreparing = useCallback(() => {
    ++wtGenRef.current;
    setTerminalPage(null);
  }, []);

  return { terminalPage, setTerminalPage, openNewTerminal, cancelPreparing };
}
