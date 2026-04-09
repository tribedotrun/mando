import React from 'react';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useWorktreeTerminal } from '#renderer/domains/terminal/hooks/useWorktreeTerminal';
import { TerminalPage } from '#renderer/domains/terminal/components/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/terminal/components/WorkspacePreparing';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

export function TerminalPageRoute(): React.ReactElement {
  const search = useRouterState({
    select: (s) => s.location.search as { project: string; cwd?: string; resume?: string },
  });

  // Key on project+resume only. Adding cwd after worktree creation must NOT remount.
  const key = `${search.project}:${search.resume ?? ''}`;
  return <TerminalPageInner key={key} search={search} />;
}

function TerminalPageInner({
  search,
}: {
  search: { project: string; cwd?: string; resume?: string };
}): React.ReactElement {
  const navigate = useNavigate();
  const { terminalPage, setTerminalPage, openNewTerminal, cancelPreparing } = useWorktreeTerminal();

  useMountEffect(() => {
    if (search.cwd) {
      setTerminalPage({
        project: search.project,
        cwd: search.cwd,
        resumeSessionId: search.resume ?? null,
      });
    } else {
      // Sync cwd into URL once worktree is ready so the sidebar can highlight.
      void openNewTerminal(search.project, (cwd) => {
        void navigate({
          to: '/terminal',
          search: { project: search.project, cwd },
          replace: true,
        });
      });
    }
  });

  if (!terminalPage) {
    return (
      <div className="flex h-full items-center justify-center pt-2 text-muted-foreground">
        Preparing terminal...
      </div>
    );
  }

  if (terminalPage.preparing) {
    return (
      <div className="h-full pt-2">
        <ErrorBoundary fallbackLabel="Terminal preparing">
          <WorkspacePreparing project={terminalPage.project} onCancel={cancelPreparing} />
        </ErrorBoundary>
      </div>
    );
  }

  return (
    <div className="h-full pt-2">
      <ErrorBoundary fallbackLabel="Terminal">
        <TerminalPage
          project={terminalPage.project}
          cwd={terminalPage.cwd}
          resumeSessionId={terminalPage.resumeSessionId}
          onResumeConsumed={() =>
            setTerminalPage((p) => (p ? { ...p, resumeSessionId: null } : null))
          }
        />
      </ErrorBoundary>
    </div>
  );
}
