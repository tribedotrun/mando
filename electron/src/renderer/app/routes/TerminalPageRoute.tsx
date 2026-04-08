import React from 'react';
import { useRouterState } from '@tanstack/react-router';
import { useWorktreeTerminal } from '#renderer/domains/terminal/hooks/useWorktreeTerminal';
import { TerminalPage } from '#renderer/domains/terminal/components/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/terminal/components/WorkspacePreparing';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

export function TerminalPageRoute(): React.ReactElement {
  const search = useRouterState({
    select: (s) => s.location.search as { project: string; cwd?: string; resume?: string },
  });

  // Force remount when search params change so useMountEffect re-fires
  const key = `${search.project}:${search.cwd ?? ''}:${search.resume ?? ''}`;
  return <TerminalPageInner key={key} search={search} />;
}

function TerminalPageInner({
  search,
}: {
  search: { project: string; cwd?: string; resume?: string };
}): React.ReactElement {
  const { terminalPage, setTerminalPage, openNewTerminal, cancelPreparing } = useWorktreeTerminal();

  useMountEffect(() => {
    if (search.cwd) {
      setTerminalPage({
        project: search.project,
        cwd: search.cwd,
        resumeSessionId: search.resume ?? null,
      });
    } else {
      void openNewTerminal(search.project);
    }
  });

  if (!terminalPage) {
    return (
      <div className="flex h-full items-center justify-center pt-[38px] text-muted-foreground">
        Preparing terminal...
      </div>
    );
  }

  if (terminalPage.preparing) {
    return (
      <div className="h-full pt-[38px]">
        <ErrorBoundary fallbackLabel="Terminal preparing">
          <WorkspacePreparing project={terminalPage.project} onCancel={cancelPreparing} />
        </ErrorBoundary>
      </div>
    );
  }

  return (
    <div className="h-full pt-[38px]">
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
