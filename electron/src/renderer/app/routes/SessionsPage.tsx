import React, { useCallback } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { SessionsCard } from '#renderer/domains/sessions/components/SessionsCard';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import type { SessionEntry } from '#renderer/types';

export function SessionsPage(): React.ReactElement {
  const navigate = useNavigate();

  const handleOpenSession = useCallback(
    (s: SessionEntry) => {
      void navigate({
        to: '/sessions/$sessionId',
        params: { sessionId: s.session_id },
        search: {
          caller: s.caller || undefined,
          cwd: s.resume_cwd || s.cwd || undefined,
          taskTitle: s.task_title || s.scout_item_title || undefined,
        },
      });
    },
    [navigate],
  );

  return (
    <div className="absolute inset-0 overflow-auto bg-background px-8 pb-6">
      <ErrorBoundary fallbackLabel="Sessions view">
        <SessionsCard active onOpenSession={handleOpenSession} />
      </ErrorBoundary>
    </div>
  );
}
