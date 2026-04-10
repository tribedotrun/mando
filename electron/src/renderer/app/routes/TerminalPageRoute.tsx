import React from 'react';
import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useWorktreeTerminal } from '#renderer/domains/terminal/hooks/useWorktreeTerminal';
import { TerminalPage } from '#renderer/domains/terminal/components/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/terminal/components/WorkspacePreparing';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import log from '#renderer/logger';

export function TerminalPageRoute(): React.ReactElement {
  const search = useRouterState({
    select: (s) =>
      s.location.search as { project: string; cwd?: string; resume?: string; name?: string },
  });

  // Key on the full terminal target so switching worktrees in the same project
  // remounts onto the correct session set, including restored placeholders.
  const key = `${search.project}:${search.cwd ?? ''}:${search.resume ?? ''}`;
  return <TerminalPageInner key={key} search={search} />;
}

function TerminalPageInner({
  search,
}: {
  search: { project: string; cwd?: string; resume?: string; name?: string };
}): React.ReactElement {
  const navigate = useNavigate();
  const { terminalPage, setTerminalPage, openNewTerminal, cancelPreparing } = useWorktreeTerminal();

  useMountEffect(() => {
    log.info('[terminal-route] mount', { project: search.project, cwd: search.cwd });
    if (!search.project) {
      log.warn('[terminal-route] project empty after beforeLoad guard — race condition');
      return;
    }
    if (search.cwd) {
      setTerminalPage({
        project: search.project,
        cwd: search.cwd,
        resumeSessionId: search.resume ?? null,
        name: search.name ?? null,
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
          resumeName={terminalPage.name}
          onResumeConsumed={() =>
            setTerminalPage((p) => (p ? { ...p, resumeSessionId: null, name: null } : null))
          }
        />
      </ErrorBoundary>
    </div>
  );
}
