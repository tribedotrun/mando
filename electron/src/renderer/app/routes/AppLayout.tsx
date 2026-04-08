import React, { useCallback, useState } from 'react';
import { Outlet, useNavigate, useMatchRoute, useRouterState } from '@tanstack/react-router';
import { useDataContext } from '#renderer/app/DataProvider';
import { useUIStore } from '#renderer/app/uiStore';
import { useSetupProgress } from '#renderer/app/useSetupProgress';
import { Sidebar, type Tab } from '#renderer/app/Sidebar';
import { RetryButton } from '#renderer/domains/captain/components/RetryButton';
import { Button } from '#renderer/components/ui/button';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useWorkbenchStore } from '#renderer/domains/terminal/stores/workbenchStore';
import { apiPost, apiPatch, apiDel } from '#renderer/api';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { useDefaultLayout } from 'react-resizable-panels';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '#renderer/components/ui/resizable';

export function AppLayout(): React.ReactElement {
  const navigate = useNavigate();
  const matchRoute = useMatchRoute();
  const { sseStatus } = useDataContext();
  const [setupActive, setSetupActive] = useState(false);
  const setupProgress = useSetupProgress();
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({
    id: 'sidebar-layout',
    storage: localStorage,
  });

  // Derive state from URL
  const projectFilter = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });
  const activeTerminalCwd = useRouterState({
    select: (s) =>
      s.location.pathname === '/terminal'
        ? ((s.location.search as { cwd?: string }).cwd ?? null)
        : null,
  });

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
      void useSettingsStore.getState().load();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Failed to add project'));
    }
  }, []);

  const handleDismissSetup = useCallback(() => {
    setSetupActive(false);
    const store = useSettingsStore.getState();
    store.updateSection('features', { setupDismissed: true });
    void store.save();
  }, []);

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
        <ResizablePanel id="sidebar" defaultSize="200px" minSize="160px" maxSize="400px">
          <Sidebar
            activeTab={activeTab}
            onTabChange={(tab) => {
              const routes: Record<Tab, string> = {
                captain: '/captain',
                scout: '/scout',
                sessions: '/sessions',
              };
              void navigate({ to: routes[tab] });
            }}
            onNewTask={() => {
              void navigate({ to: '/captain' });
              useUIStore.getState().openCreateTask();
            }}
            onOpenSettings={() =>
              void navigate({ to: '/settings/$section', params: { section: 'general' } })
            }
            onAddProject={() => void handleAddProject()}
            onRenameProject={async (oldName, newName) => {
              try {
                await apiPatch(`/api/projects/${encodeURIComponent(oldName)}`, { rename: newName });
                await useSettingsStore.getState().load();
                if (projectFilter === oldName) {
                  void navigate({ to: '/captain', search: { project: newName } });
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
                await useSettingsStore.getState().load();
                if (res.deleted_tasks > 0) await useTaskStore.getState().fetch();
                if (projectFilter === name) {
                  void navigate({ to: '/captain', search: {} });
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
                to: '/captain',
                search: project ? { project } : {},
              });
            }}
            setupProgress={setupProgress}
            setupActive={setupActive}
            onNewTerminal={(project) => void navigate({ to: '/terminal', search: { project } })}
            onOpenTask={(id) =>
              void navigate({ to: '/captain/tasks/$taskId', params: { taskId: String(id) } })
            }
            activeTerminalCwd={activeTerminalCwd}
            onOpenTerminalSession={(session) =>
              void navigate({
                to: '/terminal',
                search: { project: session.project, cwd: session.cwd },
              })
            }
            onArchiveWorkbench={(id) => {
              useWorkbenchStore
                .getState()
                .archive(id)
                .catch((err) => toast.error(getErrorMessage(err, 'Failed to archive workbench')));
            }}
          />
        </ResizablePanel>

        <ResizableHandle />

        <ResizablePanel id="main" minSize="50%">
          <main className="relative h-full overflow-hidden bg-background">
            <Outlet />
          </main>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
