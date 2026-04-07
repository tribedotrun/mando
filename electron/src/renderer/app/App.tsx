import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useDataContext } from '#renderer/app/DataProvider';
import { useGlobalKeyboard } from '#renderer/global/hooks/useKeyboardShortcuts';
import { useTaskActions } from '#renderer/domains/captain/hooks/useTaskActions';
import { Sidebar, type Tab } from '#renderer/app/Sidebar';
import { CaptainView } from '#renderer/domains/captain/components/CaptainView';
import { ScoutPage } from '#renderer/domains/scout/components/ScoutPage';
import { SessionsCard } from '#renderer/domains/sessions/components/SessionsCard';
import {
  SettingsPage,
  type SettingsSection,
} from '#renderer/domains/settings/components/SettingsPage';
import { DevInfoBar } from '#renderer/global/components/DevInfoBar';
import { CommandPalette } from '#renderer/global/components/CommandPalette';
import { BulkCreateProgress } from '#renderer/domains/captain/components/BulkCreateProgress';
import { CreateTaskModal } from '#renderer/domains/captain/components/AddTaskForm';
import { MergeModal } from '#renderer/domains/captain/components/MergeModal';
import { ShortcutOverlay } from '#renderer/global/components/ShortcutOverlay';
import { TaskDetailView } from '#renderer/domains/captain/components/TaskDetailView';
import { TerminalPage } from '#renderer/domains/terminal/components/TerminalPage';
import { RetryButton } from '#renderer/domains/captain/components/RetryButton';
import { Button } from '#renderer/components/ui/button';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import { useSetupProgress } from '#renderer/app/useSetupProgress';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { apiPost, apiPatch, apiDel } from '#renderer/api';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { usePanelRef, useDefaultLayout } from 'react-resizable-panels';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '#renderer/components/ui/resizable';
import log from '#renderer/logger';

export function App(): React.ReactElement {
  const [activeTab, setActiveTab] = useState<Tab>('captain');
  const [showSettings, setShowSettings] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('general');
  const [projectFilter, setProjectFilter] = useState<string | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [createTaskOpen, setCreateTaskOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [detailItemId, setDetailItemId] = useState<number | null>(null);
  const [setupActive, setSetupActive] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const sidebarPanelRef = usePanelRef();
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({
    id: 'sidebar-layout',
    storage: localStorage,
  });
  const [terminalPage, setTerminalPage] = useState<{
    project: string;
    cwd: string;
    label: string;
    resumeSessionId?: string | null;
  } | null>(null);

  const actions = useTaskActions();

  // Derive detailItem from the store so optimistic updates (e.g. status ->
  // captain-merging after merge) are reflected immediately, no stale snapshot.
  const detailItem = useTaskStore((s) =>
    detailItemId !== null ? (s.items.find((t) => t.id === detailItemId) ?? null) : null,
  );

  const { sseStatus } = useDataContext();

  // Load config on mount.
  const settingsLoad = useSettingsStore((s) => s.load);
  const setupProgress = useSetupProgress();

  useMountEffect(() => {
    settingsLoad().then(() => {
      // Eagerly verify Claude Code so the sidebar progress is accurate on
      // first render, rather than waiting for the checklist popover to open.
      const store = useSettingsStore.getState();
      if (store.config.features?.claudeCodeVerified || store.config.features?.setupDismissed)
        return;
      window.mandoAPI
        ?.checkClaudeCode?.()
        .then((result) => {
          if (result.installed && result.works) {
            const s = useSettingsStore.getState();
            if (!s.config.features?.claudeCodeVerified) {
              s.updateSection('features', { claudeCodeVerified: true });
              s.save();
            }
          }
        })
        .catch((err) => log.warn('eager CC check failed:', err));
    });
  });

  const handleDismissSetup = useCallback(() => {
    setSetupActive(false);
    const store = useSettingsStore.getState();
    store.updateSection('features', { setupDismissed: true });
    store.save();
  }, []);

  const handleShortcut = useCallback((action: string) => {
    switch (action) {
      case 'add-task':
        setShowSettings(false);
        setSettingsSection('general');
        setActiveTab('captain');
        setCreateTaskOpen(true);
        break;
    }
  }, []);

  const openCreateTask = useCallback(() => {
    setShowSettings(false);
    setActiveTab('captain');
    setCreateTaskOpen(true);
  }, []);

  const handleNewTerminal = useCallback((project: string) => {
    const cfg = useSettingsStore.getState().config;
    const pc = cfg.captain?.projects
      ? Object.values(cfg.captain.projects).find((p) => p.name === project)
      : undefined;
    if (!pc?.path) {
      toast.error(`No path configured for project "${project}"`);
      return;
    }
    setDetailItemId(null);
    setTerminalPage({ project, cwd: pc.path, label: `${project} / terminal` });
  }, []);

  const toggleSidebar = useCallback(() => {
    const panel = sidebarPanelRef.current;
    if (panel) {
      if (panel.isCollapsed()) panel.expand();
      else panel.collapse();
    }
  }, []);

  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'b') {
        e.preventDefault();
        toggleSidebar();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  useGlobalKeyboard({
    paletteOpen,
    shortcutsOpen,
    showSettings,
    modalOpen: createTaskOpen,
    onNavigate: setActiveTab,
    onTogglePalette: useCallback(() => setPaletteOpen((v) => !v), []),
    onOpenSettings: useCallback(() => {
      setPaletteOpen(false);
      setSettingsSection('general');
      setShowSettings(true);
    }, []),
    onToggleShortcuts: useCallback(() => setShortcutsOpen((v) => !v), []),
  });

  const handlePaletteAction = useCallback((action: string) => {
    setPaletteOpen(false);
    switch (action) {
      case 'nav-captain':
        setActiveTab('captain');
        break;
      case 'nav-scout':
      case 'recent-scout':
        setActiveTab('scout');
        break;
      case 'nav-sessions':
        setActiveTab('sessions');
        break;
      case 'act-settings':
        setSettingsSection('general');
        setShowSettings(true);
        break;
      case 'act-create-task':
        setActiveTab('captain');
        setCreateTaskOpen(true);
        break;
    }
  }, []);

  useMountEffect(() => {
    if (window.mandoAPI) {
      window.mandoAPI.onShortcut(handleShortcut);
      return () => window.mandoAPI.removeShortcutListeners();
    }
  });

  // Navigate to the relevant view when a desktop notification is clicked.
  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.onNotificationClick((data) => {
      const kind = data.kind as { type: string } | undefined;
      setShowSettings(false);

      // Task-related notifications, open task detail.
      if (data.item_id) {
        const id = Number(data.item_id);
        if (!Number.isNaN(id)) {
          const task = useTaskStore.getState().items.find((t) => t.id === id);
          if (task) {
            setActiveTab('captain');
            setDetailItemId(id);
            return;
          }
        }
      }

      // RateLimited, captain tab (where impact is visible).
      // Clear detailItem so the tab switch is visible (detail view short-circuits render).
      if (kind?.type === 'RateLimited') {
        setDetailItemId(null);
        setActiveTab('captain');
      }
      // Generic: window is already shown/focused by the main process.
    });
    return () => window.mandoAPI.removeNotificationClickListeners();
  });

  if (detailItem) {
    return (
      <div className="flex h-screen flex-col bg-background">
        <div className="h-8 shrink-0" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
        <div className="relative flex-1 overflow-hidden">
          {terminalPage ? (
            <div className="absolute inset-0 z-[2] bg-background">
              <TerminalPage
                project={terminalPage.project}
                cwd={terminalPage.cwd}
                label={terminalPage.label}
                resumeSessionId={terminalPage.resumeSessionId}
                onResumeConsumed={() =>
                  setTerminalPage((p) => (p ? { ...p, resumeSessionId: null } : null))
                }
                onBack={() => setTerminalPage(null)}
              />
            </div>
          ) : (
            <div className="h-full px-8 py-4">
              <ErrorBoundary fallbackLabel="Task detail">
                <TaskDetailView
                  item={detailItem}
                  onBack={() => {
                    actions.setMergeItem(null);
                    setDetailItemId(null);
                  }}
                  onMerge={() => actions.setMergeItem(detailItem)}
                  onOpenTerminal={(opts) => setTerminalPage(opts)}
                />
              </ErrorBoundary>
            </div>
          )}
        </div>
        <DevInfoBar />
        {actions.mergeItem && (
          <MergeModal
            item={actions.mergeItem}
            onConfirm={actions.handleMerge}
            onCancel={() => actions.setMergeItem(null)}
            pending={actions.mergePending}
            result={actions.mergeResult}
          />
        )}
        <CommandPalette
          open={paletteOpen}
          onClose={() => setPaletteOpen(false)}
          onAction={handlePaletteAction}
        />
        <CreateTaskModal
          open={createTaskOpen}
          onClose={() => setCreateTaskOpen(false)}
          initialProject={projectFilter}
        />
        <ShortcutOverlay open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
        <BulkCreateProgress />
      </div>
    );
  }

  return (
    <div className="relative flex h-screen flex-col bg-background">
      {/* Title bar drag region, absolute so it doesn't push content down */}
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />

      {showSettings && (
        <div className="flex-1 overflow-hidden">
          <SettingsPage
            onBack={() => {
              setShowSettings(false);
              setSettingsSection('general');
            }}
            initialSection={settingsSection}
          />
        </div>
      )}

      {/* Main layout, hidden (not unmounted) when settings is open */}
      <div className={`flex min-h-0 flex-1 flex-col${showSettings ? ' hidden' : ''}`}>
        {/* Disconnected banner, mt-8 clears absolute drag region */}
        {sseStatus === 'disconnected' && (
          <div className="flex h-10 shrink-0 items-center gap-3 px-4 mt-8 bg-card">
            <span className="h-2 w-2 shrink-0 rounded-full bg-stale" />
            <span className="text-body font-medium text-foreground">Daemon disconnected</span>
            <span className="text-caption text-muted-foreground">Reconnecting&hellip;</span>
            <span className="flex-1" />
            <RetryButton
              className="inline-flex items-center justify-center rounded-md bg-secondary px-3 py-1 text-[13px] font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground"
              onRetry={() =>
                window.mandoAPI.restartDaemon().finally(() => window.location.reload())
              }
            />
            <Button
              variant="link"
              size="xs"
              className="text-caption text-muted-foreground hover:text-foreground"
              onClick={() => window.mandoAPI.openLogsFolder()}
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
            id="sidebar"
            panelRef={sidebarPanelRef}
            defaultSize="200px"
            minSize="160px"
            maxSize="400px"
            collapsible
            collapsedSize="32px"
            onResize={(size) => setSidebarCollapsed(size.inPixels < 33)}
          >
            <Sidebar
              activeTab={activeTab}
              onTabChange={setActiveTab}
              onNewTask={() => {
                setActiveTab('captain');
                setCreateTaskOpen(true);
              }}
              onOpenSettings={() => {
                setSettingsSection('general');
                setShowSettings(true);
              }}
              onAddProject={async () => {
                const dir = await window.mandoAPI.selectDirectory();
                if (!dir) return;
                try {
                  await apiPost('/api/projects', { path: dir });
                  useSettingsStore.getState().load();
                } catch (err) {
                  toast.error(getErrorMessage(err, 'Failed to add project'));
                }
              }}
              onRenameProject={async (oldName, newName) => {
                try {
                  await apiPatch(`/api/projects/${encodeURIComponent(oldName)}`, {
                    rename: newName,
                  });
                  await useSettingsStore.getState().load();
                  setProjectFilter((prev) => (prev === oldName ? newName : prev));
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
                  if (res.deleted_tasks > 0) {
                    await useTaskStore.getState().fetch();
                  }
                  setProjectFilter((prev) => (prev === name ? null : prev));
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
              onProjectFilter={setProjectFilter}
              setupProgress={setupProgress}
              setupActive={setupActive}
              collapsed={sidebarCollapsed}
              onToggleCollapse={toggleSidebar}
              onNewTerminal={handleNewTerminal}
              onOpenTask={(id) => setDetailItemId(id)}
            />
          </ResizablePanel>

          <ResizableHandle />

          <ResizablePanel id="main" minSize="50%">
            <main className="relative h-full overflow-hidden bg-background">
              {terminalPage && (
                <div className="absolute inset-0 z-[2] overflow-hidden bg-background pt-[38px]">
                  <TerminalPage
                    project={terminalPage.project}
                    cwd={terminalPage.cwd}
                    label={terminalPage.label}
                    resumeSessionId={terminalPage.resumeSessionId}
                    onResumeConsumed={() =>
                      setTerminalPage((p) => (p ? { ...p, resumeSessionId: null } : null))
                    }
                    onBack={() => setTerminalPage(null)}
                  />
                </div>
              )}

              {/* All tabs stay mounted and stacked. Active tab sits on top via
                z-index; inactive tabs are behind the opaque background, no
                visibility/display changes, so CSS transitions can't flash. */}
              {(['captain', 'scout', 'sessions'] as const).map((tab) => {
                const isActive = activeTab === tab;
                return (
                  <div
                    key={tab}
                    className={`absolute inset-0 overflow-auto bg-background px-8 pb-6 pt-[38px]${isActive ? ' z-[1]' : ' z-0 pointer-events-none'}`}
                  >
                    {tab === 'captain' && (
                      <ErrorBoundary fallbackLabel="Captain view">
                        <CaptainView
                          projectFilter={projectFilter}
                          onCreateTask={openCreateTask}
                          onOpenDetail={(item) => setDetailItemId(item.id)}
                          active={isActive}
                        />
                      </ErrorBoundary>
                    )}
                    {tab === 'scout' && (
                      <ErrorBoundary fallbackLabel="Scout view">
                        <ScoutPage active={isActive} />
                      </ErrorBoundary>
                    )}
                    {tab === 'sessions' && (
                      <ErrorBoundary fallbackLabel="Sessions view">
                        <SessionsCard active={isActive} />
                      </ErrorBoundary>
                    )}
                  </div>
                );
              })}
            </main>
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <DevInfoBar />

      {/* Overlays */}
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onAction={handlePaletteAction}
      />
      <CreateTaskModal
        open={createTaskOpen}
        onClose={() => setCreateTaskOpen(false)}
        initialProject={projectFilter}
      />
      <ShortcutOverlay open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      <BulkCreateProgress />
    </div>
  );
}
