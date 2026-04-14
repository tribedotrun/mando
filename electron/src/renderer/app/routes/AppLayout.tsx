import React, { useCallback, useState } from 'react';
import { Outlet, useNavigate, useMatchRoute, useRouterState } from '@tanstack/react-router';
import { usePanelRef, useDefaultLayout } from 'react-resizable-panels';
import { useDataContext } from '#renderer/app/DataProvider';
import { useUIStore } from '#renderer/app/uiStore';
import { useSetupProgress } from '#renderer/app/useSetupProgress';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { Sidebar, type Tab } from '#renderer/app/Sidebar';
import { RetryButton } from '#renderer/domains/captain/components/RetryButton';
import { Button } from '#renderer/components/ui/button';
import { useQueryClient } from '@tanstack/react-query';
import { useConfigSave, useWorkbenchArchive } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import type { MandoConfig, TaskListResponse } from '#renderer/types';
import { createWorktree, type WorkbenchItem } from '#renderer/api-terminal';
import { apiPost, apiPatch, apiDel } from '#renderer/api';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '#renderer/components/ui/resizable';
import { AppHeader } from '#renderer/app/AppHeader';
import { router } from '#renderer/app/router';
import log from '#renderer/logger';

export function AppLayout(): React.ReactElement {
  const navigate = useNavigate();
  const sidebarRef = usePanelRef();
  const matchRoute = useMatchRoute();
  const { sseStatus } = useDataContext();
  const [setupActive, setSetupActive] = useState(false);
  const setupProgress = useSetupProgress();
  const archiveWorkbench = useWorkbenchArchive();
  const saveMut = useConfigSave();
  const qc = useQueryClient();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [preparingProject, setPreparingProject] = useState<string | null>(null);
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({
    id: 'sidebar-layout',
    storage: localStorage,
  });

  // Toggle sidebar and explicitly sync collapsed state.
  const handleToggleSidebar = useCallback(() => {
    const panel = sidebarRef.current;
    if (!panel) return;
    if (panel.isCollapsed()) {
      panel.expand();
      setSidebarCollapsed(false);
    } else {
      panel.collapse();
      setSidebarCollapsed(true);
    }
  }, [sidebarRef]);

  const handleGoBack = useCallback(() => router.history.back(), []);
  const handleGoForward = useCallback(() => router.history.forward(), []);
  const handleNewTask = useCallback(() => useUIStore.getState().openCreateTask(), []);

  // Sync initial collapsed state (panel may restore collapsed from localStorage)
  useMountEffect(() => {
    if (sidebarRef.current?.isCollapsed()) setSidebarCollapsed(true);
  });

  // Listen for global sidebar toggle shortcut (Cmd+B dispatched from useGlobalKeyboard)
  useMountEffect(() => {
    window.addEventListener('mando:toggle-sidebar', handleToggleSidebar);
    return () => window.removeEventListener('mando:toggle-sidebar', handleToggleSidebar);
  });

  // Derive state from URL
  const projectFilter = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });
  const activeWorkbenchId = useRouterState({
    select: (s) => {
      const m = s.location.pathname.match(/^\/wb\/(\w+)/);
      return m ? m[1] : null;
    },
  });
  const activeTaskId = useRouterState({
    select: (s) => {
      const wbMatch = s.location.pathname.match(/^\/wb\/(\d+)/);
      if (!wbMatch) return null;
      const wbId = Number(wbMatch[1]);
      const taskData = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const task = taskData?.items.find((t) => t.workbench_id === wbId);
      return task?.id ?? null;
    },
  });

  // Resolve active workbench's worktree path for sidebar highlighting
  const allWorkbenches = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list()) ?? [];
  const numericWbId = activeWorkbenchId ? Number(activeWorkbenchId) : null;
  const activeWorktreeCwd =
    numericWbId && !Number.isNaN(numericWbId)
      ? (allWorkbenches.find((w) => w.id === numericWbId)?.worktree ?? null)
      : null;

  // Derive activeTab from current route
  const activeTab: Tab = matchRoute({ to: '/scout', fuzzy: true })
    ? 'scout'
    : matchRoute({ to: '/sessions', fuzzy: true })
      ? 'sessions'
      : 'captain';

  const handleAddProject = useCallback(async () => {
    try {
      const dir = await window.mandoAPI.selectDirectory();
      if (!dir) return;
      await apiPost('/api/projects', { path: dir });
      void qc.invalidateQueries({ queryKey: queryKeys.config.all });
    } catch (err) {
      toast.error(getErrorMessage(err, 'Failed to add project'));
    }
  }, [qc]);

  const handleDismissSetup = useCallback(() => {
    setSetupActive(false);
    const current = qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? {};
    const updated: MandoConfig = {
      ...current,
      features: { ...(current.features || {}), setupDismissed: true },
    };
    saveMut.mutate(updated);
  }, [qc, saveMut]);

  // Navigate to a workbench by its ID
  const navigateToWorkbench = useCallback(
    (wbId: number, tab?: string) => {
      void navigate({
        to: '/wb/$workbenchId',
        params: { workbenchId: String(wbId) },
        search: tab ? { tab } : {},
      });
    },
    [navigate],
  );

  // Resolve workbench ID from a task ID
  const openTaskWorkbench = useCallback(
    (taskId: number) => {
      const taskData = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const task = taskData?.items.find((t) => t.id === taskId);
      if (task?.workbench_id) {
        navigateToWorkbench(task.workbench_id);
      } else {
        void navigate({ to: '/' });
      }
    },
    [qc, navigateToWorkbench, navigate],
  );

  // Resolve workbench ID from a worktree path
  const openWorktreeWorkbench = useCallback(
    (cwd: string) => {
      const workbenches = qc.getQueryData<WorkbenchItem[]>(queryKeys.workbenches.list()) ?? [];
      const wb = workbenches.find((w) => w.worktree === cwd);
      if (wb) {
        navigateToWorkbench(wb.id, 'terminal');
      }
    },
    [qc, navigateToWorkbench],
  );

  // Create a new workbench (pen icon)
  const handleNewTerminal = useCallback(
    async (project: string) => {
      if (preparingProject) return;
      setPreparingProject(project);
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
        void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
        if (result.workbenchId) {
          navigateToWorkbench(result.workbenchId, 'terminal');
        }
      } catch (err) {
        log.error('createWorktree failed', err);
        toast.error(getErrorMessage(err, 'Failed to create workspace'));
      } finally {
        setPreparingProject(null);
      }
    },
    [preparingProject, qc, navigateToWorkbench],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Disconnected banner */}
      {sseStatus === 'disconnected' && (
        <div className="mt-8 flex h-10 shrink-0 items-center gap-3 bg-card px-4">
          <span className="h-2 w-2 shrink-0 rounded-full bg-stale" />
          <span className="text-body font-medium text-foreground">Daemon disconnected</span>
          <span className="text-caption text-muted-foreground">Reconnecting&hellip;</span>
          <span className="flex-1" />
          <RetryButton
            className="inline-flex items-center justify-center rounded-md bg-secondary px-3 py-1 text-[13px] font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            onRetry={() =>
              void window.mandoAPI.restartDaemon().finally(() => window.location.reload())
            }
          />
          <Button
            variant="link"
            size="xs"
            className="text-caption text-muted-foreground hover:text-foreground"
            onClick={() => void window.mandoAPI.openLogsFolder()}
          >
            View logs
          </Button>
        </div>
      )}

      <ResizablePanelGroup
        orientation="horizontal"
        defaultLayout={defaultLayout}
        onLayoutChanged={onLayoutChanged}
        className="min-h-0 flex-1"
      >
        <ResizablePanel
          panelRef={sidebarRef}
          id="sidebar"
          defaultSize="200px"
          minSize="160px"
          maxSize="400px"
          collapsible
          collapsedSize="0px"
          onResize={() => setSidebarCollapsed(sidebarRef.current?.isCollapsed() ?? false)}
        >
          <Sidebar
            activeTab={activeTab}
            onTabChange={(tab) => {
              const routes: Record<Tab, string> = {
                captain: '/',
                scout: '/scout',
                sessions: '/sessions',
              };
              void navigate({ to: routes[tab] });
            }}
            onNewTask={() => {
              useUIStore.getState().openCreateTask();
            }}
            onOpenSettings={() =>
              void navigate({ to: '/settings/$section', params: { section: 'general' } })
            }
            onAddProject={() => void handleAddProject()}
            onRenameProject={async (oldName, newName) => {
              try {
                await apiPatch(`/api/projects/${encodeURIComponent(oldName)}`, { rename: newName });
                void qc.invalidateQueries({ queryKey: queryKeys.config.all });
                if (projectFilter === oldName) {
                  void navigate({ to: '/', search: { project: newName } });
                }
                toast.success(`Renamed to "${newName}"`);
              } catch (err) {
                toast.error(getErrorMessage(err, 'Failed to rename project'));
              }
            }}
            onRemoveProject={async (name) => {
              try {
                const res = await apiDel<{ ok: boolean; deleted_tasks: number }>(
                  `/api/projects/${encodeURIComponent(name)}`,
                );
                void qc.invalidateQueries({ queryKey: queryKeys.config.all });
                if (projectFilter === name) {
                  void navigate({ to: '/', search: {} });
                }
                const taskMsg =
                  res.deleted_tasks > 0
                    ? ` and ${res.deleted_tasks} task${res.deleted_tasks !== 1 ? 's' : ''}`
                    : '';
                toast.success(`Deleted "${name}"${taskMsg}`);
              } catch (err) {
                toast.error(getErrorMessage(err, 'Failed to remove project'));
              }
            }}
            onToggleSetup={() => setSetupActive((v) => !v)}
            onDismissSetup={handleDismissSetup}
            projectFilter={projectFilter}
            onProjectFilter={(project) => {
              void navigate({
                to: '/',
                search: project ? { project } : {},
              });
            }}
            setupProgress={setupProgress}
            setupActive={setupActive}
            onNewTerminal={(project) => void handleNewTerminal(project)}
            onOpenTask={(id) => openTaskWorkbench(id)}
            activeTerminalCwd={activeWorktreeCwd}
            activeTaskId={activeTaskId}
            onOpenTerminalSession={(session) => openWorktreeWorkbench(session.cwd)}
            onArchiveWorkbench={(id) => {
              archiveWorkbench.mutate({ id });
            }}
            onToggleSidebar={handleToggleSidebar}
            onGoBack={() => router.history.back()}
            onGoForward={() => router.history.forward()}
          />
        </ResizablePanel>

        <ResizableHandle />

        <ResizablePanel id="main" minSize="50%">
          <main className="flex h-full flex-col overflow-hidden bg-background">
            <AppHeader
              sidebarCollapsed={sidebarCollapsed}
              onToggleSidebar={handleToggleSidebar}
              onGoBack={handleGoBack}
              onGoForward={handleGoForward}
              onNewTask={handleNewTask}
            />
            <div className="relative min-h-0 flex-1">
              <Outlet />
            </div>
          </main>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
