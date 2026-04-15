import React, { useCallback, useRef, useState } from 'react';
import { useNavigate, useParams, useSearch } from '@tanstack/react-router';
import { useTaskList, useWorkbenchList } from '#renderer/hooks/queries';
import { useUIStore } from '#renderer/app/uiStore';
import { TaskDetailView } from '#renderer/domains/captain/components/TaskDetailView';
import { TerminalPage } from '#renderer/domains/terminal/components/TerminalPage';
import { WorkspacePreparing } from '#renderer/domains/terminal/components/WorkspacePreparing';
import { useWorktreeTerminal } from '#renderer/domains/terminal/hooks/useWorktreeTerminal';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

export function WorkbenchPage(): React.ReactElement {
  const navigate = useNavigate();
  const { workbenchId } = useParams({ strict: false }) as { workbenchId: string };
  const search = useSearch({ strict: false }) as {
    tab?: string;
    resume?: string;
    name?: string;
    project?: string;
  };
  const isNewWorkbench = workbenchId === 'new';
  const wbId = isNewWorkbench ? null : Number(workbenchId);

  // Key the TerminalPage so it remounts when a resume is requested.
  // Ref persists the key after the resume param is consumed from the URL.
  const terminalKeyRef = useRef(search.resume ?? '');
  if (search.resume && search.resume !== terminalKeyRef.current) {
    terminalKeyRef.current = search.resume;
  }
  const terminalKey = terminalKeyRef.current;

  // For "new" workbench creation flow
  const { terminalPage, openNewTerminal, cancelPreparing } = useWorktreeTerminal();
  const creationStarted = useRef(false);

  useMountEffect(() => {
    if (isNewWorkbench && search.project && !creationStarted.current) {
      creationStarted.current = true;
      void openNewTerminal(search.project, (_cwd, result) => {
        if (result?.workbenchId) {
          void navigate({
            to: '/wb/$workbenchId',
            params: { workbenchId: String(result.workbenchId) },
            search: { tab: 'terminal' },
            replace: true,
          });
        }
      });
    }
  });

  // Use active list (Tier 1, zero refetch) as primary source.
  // Only fetch 'all' when the workbench isn't in the active cache (archived).
  const { data: activeWbs = [], isLoading: activeLoading } = useWorkbenchList();
  const activeMatch = wbId ? (activeWbs.find((w) => w.id === wbId) ?? null) : null;
  const { data: allWbs = [], isLoading: allLoading } = useWorkbenchList(
    wbId && !activeMatch ? 'all' : undefined,
  );
  const workbenchesLoading = activeLoading || (!activeMatch && allLoading);
  const { data: taskData, isLoading: tasksLoading } = useTaskList();
  const workbench = activeMatch ?? (wbId ? (allWbs.find((w) => w.id === wbId) ?? null) : null);
  const task = taskData?.items.find((t) => t.workbench_id === wbId) ?? null;

  const handleBack = useCallback(() => {
    useUIStore.getState().setMergeItem(null);
    void navigate({ to: '/' });
  }, [navigate]);

  const handleOpenTranscript = useCallback(
    (opts: {
      sessionId: string;
      caller?: string;
      cwd?: string;
      project?: string;
      taskTitle?: string;
    }) => {
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

  // Handle cancel for new workbench creation
  const handleCancelNew = useCallback(() => {
    cancelPreparing();
    void navigate({ to: '/', replace: true });
  }, [cancelPreparing, navigate]);

  // New workbench creation flow
  if (isNewWorkbench) {
    if (terminalPage?.preparing) {
      return (
        <div className="h-full px-3 pt-2">
          <ErrorBoundary fallbackLabel="Workspace preparing">
            <WorkspacePreparing project={search.project ?? ''} onCancel={handleCancelNew} />
          </ErrorBoundary>
        </div>
      );
    }
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        Preparing workspace...
      </div>
    );
  }

  // Existing workbench
  if (!workbench) {
    return (
      <div className="flex h-full items-center justify-center text-muted-foreground">
        {tasksLoading || workbenchesLoading ? 'Loading...' : 'Workbench not found'}
      </div>
    );
  }

  // Taskless workbench: render terminal directly
  if (!task) {
    return (
      <div className="h-full px-3 pt-2">
        <ErrorBoundary fallbackLabel="Terminal">
          <TerminalPage
            key={`terminal-${workbench.id}-${terminalKey}`}
            project={workbench.project}
            cwd={workbench.worktree}
            resumeSessionId={search.resume}
            resumeName={search.name}
            onResumeConsumed={handleResumeConsumed}
          />
        </ErrorBoundary>
      </div>
    );
  }

  // Task workbench: lazy-mount terminal only after user visits the terminal tab.
  // This prevents eagerly creating terminal sessions on every task navigation.
  const terminalSlot = terminalVisited ? (
    <TerminalPage
      key={`terminal-${workbench.id}-${terminalKey}`}
      project={workbench.project}
      cwd={workbench.worktree}
      resumeSessionId={search.tab === 'terminal' ? (search.resume ?? null) : null}
      resumeName={search.tab === 'terminal' ? (search.name ?? null) : null}
      onResumeConsumed={handleResumeConsumed}
    />
  ) : null;

  return (
    <div className="h-full px-3 pt-2">
      <ErrorBoundary fallbackLabel="Workbench">
        <TaskDetailView
          item={task}
          onBack={handleBack}
          onOpenTranscript={handleOpenTranscript}
          activeTab={search.tab}
          onTabChange={handleTabChange}
          onResumeInTerminal={handleResumeInTerminal}
          terminalSlot={terminalSlot}
        />
      </ErrorBoundary>
    </div>
  );
}
