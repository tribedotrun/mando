import { useCallback } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import type { TaskProvider } from '#renderer/global/types';

type OpenTranscriptOpts = {
  sessionId: string;
  caller?: string;
  cwd?: string;
  provider?: TaskProvider;
  project?: string;
  taskTitle?: string;
};

export function useWorkbenchNav(workbenchId: string) {
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
          provider: opts.provider,
          project: opts.project,
          taskTitle: opts.taskTitle,
        },
      });
    },
    [navigate],
  );

  const handleTabChange = useCallback(
    (newTab: string) => {
      void navigate({
        to: '/wb/$workbenchId',
        params: { workbenchId },
        search: { tab: newTab },
        replace: true,
      });
    },
    [navigate, workbenchId],
  );

  return {
    handleBack,
    handleOpenTranscript,
    handleTabChange,
  };
}
