import { useCallback, useRef, useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { useUIStore } from '#renderer/global/runtime/useUIStore';

type OpenTranscriptOpts = {
  sessionId: string;
  caller?: string;
  cwd?: string;
  project?: string;
  taskTitle?: string;
};

export function useWorkbenchNav(workbenchId: string, search: { tab?: string; resume?: string }) {
  const navigate = useNavigate();

  const handleBack = useCallback(() => {
    useUIStore.getState().setMergeItem(null);
    void navigate({ to: '/' });
  }, [navigate]);

  const handleOpenTranscript = useCallback(
    (opts: OpenTranscriptOpts) => {
      void navigate({
        to: '/sessions/$sessionId',
        params: { sessionId: opts.sessionId },
        search: {
          caller: opts.caller,
          cwd: opts.cwd,
          project: opts.project,
          taskTitle: opts.taskTitle,
        },
      });
    },
    [navigate],
  );

  // Track whether the terminal tab has been visited so we can lazy-mount
  // TerminalPage and avoid eagerly creating terminal sessions.
  // Reset when TanStack Router reuses the component for a different workbench.
  const [terminalVisited, setTerminalVisited] = useState(
    search.tab === 'terminal' || !!search.resume,
  );
  const prevWbRef = useRef(workbenchId);
  if (prevWbRef.current !== workbenchId) {
    prevWbRef.current = workbenchId;
    setTerminalVisited(search.tab === 'terminal' || !!search.resume);
  }

  const handleTabChange = useCallback(
    (newTab: string) => {
      if (newTab === 'terminal') setTerminalVisited(true);
      void navigate({
        to: '/wb/$workbenchId',
        params: { workbenchId },
        search: { tab: newTab },
        replace: true,
      });
    },
    [navigate, workbenchId],
  );

  const handleResumeInTerminal = useCallback(
    (sessionId: string, name?: string) => {
      setTerminalVisited(true);
      void navigate({
        to: '/wb/$workbenchId',
        params: { workbenchId },
        search: { tab: 'terminal', resume: sessionId, name },
        replace: true,
      });
    },
    [navigate, workbenchId],
  );

  const handleResumeConsumed = useCallback(() => {
    // Clear resume/name from URL
    void navigate({
      to: '/wb/$workbenchId',
      params: { workbenchId },
      search: { tab: 'terminal' },
      replace: true,
    });
  }, [navigate, workbenchId]);

  return {
    terminalVisited,
    handleBack,
    handleOpenTranscript,
    handleTabChange,
    handleResumeInTerminal,
    handleResumeConsumed,
  };
}
