import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useDataContext } from '#renderer/DataProvider';
import { useGlobalKeyboard } from '#renderer/hooks/useKeyboardShortcuts';
import { useTaskActions } from '#renderer/hooks/useTaskActions';
import { Sidebar, type Tab, type SetupProgress } from '#renderer/components/Sidebar';
import { CaptainView } from '#renderer/components/CaptainView';
import { ScoutPage } from '#renderer/components/ScoutPage';
import { SessionsCard } from '#renderer/components/SessionsCard';
import { SettingsPage, type SettingsSection } from '#renderer/components/SettingsPage';
import { DevInfoBar } from '#renderer/components/DevInfoBar';
import { CommandPalette } from '#renderer/components/CommandPalette';
import { ToastContainer } from '#renderer/components/ToastContainer';
import { BulkCreateProgress } from '#renderer/components/BulkCreateProgress';
import { CreateTaskModal } from '#renderer/components/AddTaskForm';
import { MergeModal } from '#renderer/components/MergeModal';
import { ShortcutOverlay } from '#renderer/components/ShortcutOverlay';
import { TaskDetailView } from '#renderer/components/TaskDetailView';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useTaskStore } from '#renderer/stores/taskStore';
import { apiPost, apiPatch, apiDel } from '#renderer/api';
import { useToastStore } from '#renderer/stores/toastStore';

const SETUP_TOTAL = 3;

const STEP_NAMES = ['Install Claude Code', 'Connect Telegram for remote control', 'Add a project'];

/**
 * Compute setup progress from config (no IPC — sidebar-safe).
 * Claude Code detection is async (IPC) so the sidebar can't check it directly.
 * We mark CC as done only after the checklist has validated it (stored in features).
 */
function useSetupProgress(): SetupProgress | null {
  const config = useSettingsStore((s) => s.config);
  const loaded = useSettingsStore((s) => s.loaded);
  const dismissed = config.features?.setupDismissed ?? false;

  if (!loaded || dismissed) return null;

  const hasProject = Object.keys(config.captain?.projects ?? {}).length > 0;
  const done = [
    !!config.features?.claudeCodeVerified,
    !!(config.channels?.telegram?.enabled && config.env?.TELEGRAM_MANDO_BOT_TOKEN),
    hasProject,
  ];

  const completed = done.filter(Boolean).length;
  if (completed >= SETUP_TOTAL) {
    if (!dismissed) {
      const store = useSettingsStore.getState();
      store.updateSection('features', { setupDismissed: true });
      store.save();
    }
    return null;
  }

  const firstIncomplete = done.findIndex((d) => !d);
  return { completed, total: SETUP_TOTAL, currentStep: STEP_NAMES[firstIncomplete] ?? '' };
}

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

  const actions = useTaskActions();

  // Derive detailItem from the store so optimistic updates (e.g. status →
  // captain-merging after merge) are reflected immediately — no stale snapshot.
  const detailItem = useTaskStore((s) =>
    detailItemId !== null ? (s.items.find((t) => t.id === detailItemId) ?? null) : null,
  );

  const { sseStatus } = useDataContext();

  // Load config on mount.
  const settingsLoad = useSettingsStore((s) => s.load);
  const setupProgress = useSetupProgress();

  useMountEffect(() => {
    settingsLoad();
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

      // Task-related notifications → open task detail.
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

      // RateLimited → captain tab (where impact is visible).
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
      <div className="flex h-screen flex-col" style={{ background: 'var(--color-bg)' }}>
        <div className="h-8 shrink-0" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
        <div className="flex-1 overflow-hidden px-8 py-4">
          <TaskDetailView
            item={detailItem}
            onBack={() => {
              actions.setMergeItem(null);
              setDetailItemId(null);
            }}
            onMerge={() => actions.setMergeItem(detailItem)}
          />
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
        <CreateTaskModal open={createTaskOpen} onClose={() => setCreateTaskOpen(false)} />
        <ShortcutOverlay open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
        <ToastContainer />
        <BulkCreateProgress />
      </div>
    );
  }

  return (
    <div className="relative flex h-screen flex-col" style={{ background: 'var(--color-bg)' }}>
      {/* Title bar drag region — absolute so it doesn't push content down */}
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />

      {/* Settings — overlays the main layout without unmounting it */}
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

      {/* Main layout — hidden (not unmounted) when settings is open */}
      <div
        className="flex min-h-0 flex-1 flex-col"
        style={{ display: showSettings ? 'none' : undefined }}
      >
        {/* Disconnected banner — mt-8 clears absolute drag region */}
        {sseStatus === 'disconnected' && (
          <div
            className="flex shrink-0 items-center gap-3 px-4 mt-8"
            style={{
              height: 40,
              background: 'var(--color-surface-1)',
              borderBottom: '1px solid var(--color-border-subtle)',
            }}
          >
            <span
              className="h-2 w-2 shrink-0 rounded-full"
              style={{ background: 'var(--color-stale)' }}
            />
            <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
              Daemon disconnected
            </span>
            <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
              Reconnecting&hellip;
            </span>
            <span className="flex-1" />
            <button
              className="rounded-md px-3 py-1 text-[13px] font-medium"
              style={{
                background: 'transparent',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-2)',
                cursor: 'pointer',
                borderRadius: 6,
              }}
              onClick={(e) => {
                const btn = e.currentTarget;
                btn.textContent = 'Retrying\u2026';
                btn.style.opacity = '0.5';
                btn.style.pointerEvents = 'none';
                window.mandoAPI.restartDaemon().finally(() => window.location.reload());
              }}
            >
              Retry
            </button>
            <button
              className="text-caption"
              style={{
                background: 'none',
                border: 'none',
                color: 'var(--color-text-3)',
                cursor: 'pointer',
                padding: 0,
              }}
              onClick={() => window.mandoAPI.openLogsFolder()}
            >
              View logs
            </button>
          </div>
        )}

        <div className="flex min-h-0 flex-1">
          {/* Sidebar */}
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
                const msg = err instanceof Error ? err.message : 'Failed to add project';
                useToastStore.getState().add('error', msg);
              }
            }}
            onRenameProject={async (oldName, newName) => {
              try {
                await apiPatch(`/api/projects/${encodeURIComponent(oldName)}`, {
                  rename: newName,
                });
                await useSettingsStore.getState().load();
                setProjectFilter((prev) => (prev === oldName ? newName : prev));
                useToastStore.getState().add('success', `Renamed to "${newName}"`);
              } catch (err) {
                const msg = err instanceof Error ? err.message : 'Failed to rename project';
                useToastStore.getState().add('error', msg);
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
                useToastStore.getState().add('success', `Deleted "${name}"${taskMsg}`);
              } catch (err) {
                const msg = err instanceof Error ? err.message : 'Failed to remove project';
                useToastStore.getState().add('error', msg);
              }
            }}
            onToggleSetup={() => setSetupActive((v) => !v)}
            onDismissSetup={handleDismissSetup}
            projectFilter={projectFilter}
            onProjectFilter={setProjectFilter}
            setupProgress={setupProgress}
            setupActive={setupActive}
          />

          {/* Main content — always visible, popover floats above from sidebar */}
          <main
            className="relative flex-1 overflow-hidden"
            style={{ background: 'var(--color-bg)' }}
          >
            {/* All tabs stay mounted and stacked. Active tab sits on top via
                z-index; inactive tabs are behind the opaque background — no
                visibility/display changes, so CSS transitions can't flash. */}
            {(['captain', 'scout', 'sessions'] as const).map((tab) => {
              const isActive = activeTab === tab;
              return (
                <div
                  key={tab}
                  className="absolute inset-0 overflow-auto"
                  style={{
                    padding: '38px 32px 24px',
                    background: 'var(--color-bg)',
                    zIndex: isActive ? 1 : 0,
                    pointerEvents: isActive ? undefined : 'none',
                  }}
                >
                  {tab === 'captain' && (
                    <CaptainView
                      projectFilter={projectFilter}
                      onCreateTask={openCreateTask}
                      onOpenDetail={(item) => setDetailItemId(item.id)}
                      active={isActive}
                    />
                  )}
                  {tab === 'scout' && <ScoutPage active={isActive} />}
                  {tab === 'sessions' && <SessionsCard active={isActive} />}
                </div>
              );
            })}
          </main>
        </div>
      </div>

      <DevInfoBar />

      {/* Overlays */}
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onAction={handlePaletteAction}
      />
      <CreateTaskModal open={createTaskOpen} onClose={() => setCreateTaskOpen(false)} />
      <ShortcutOverlay open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      <ToastContainer />
      <BulkCreateProgress />
    </div>
  );
}
