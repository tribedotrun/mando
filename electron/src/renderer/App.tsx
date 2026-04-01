import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useDataContext } from '#renderer/DataProvider';
import { useGlobalKeyboard } from '#renderer/hooks/useKeyboardShortcuts';
import { Sidebar, type Tab, type SetupProgress } from '#renderer/components/Sidebar';
import { CaptainView } from '#renderer/components/CaptainView';
import { ScoutPage } from '#renderer/components/ScoutPage';
import { SessionsCard } from '#renderer/components/SessionsCard';
import { CronJobsPanel } from '#renderer/components/CronJobsPanel';
import { AnalyticsPage } from '#renderer/components/AnalyticsPage';
import { SettingsPage } from '#renderer/components/SettingsPage';
import { DevInfoBar } from '#renderer/components/DevInfoBar';
import { CommandPalette } from '#renderer/components/CommandPalette';
import { ToastContainer } from '#renderer/components/ToastContainer';
import { CreateTaskModal } from '#renderer/components/AddTaskForm';
import { ShortcutOverlay } from '#renderer/components/ShortcutOverlay';
import { TaskDetailView } from '#renderer/components/TaskDetailView';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import type { TaskItem } from '#renderer/types';

const SETUP_TOTAL = 4;

const STEP_NAMES = [
  'Install Claude Code',
  'Connect Telegram for remote control',
  'Add a project',
  'Connect Linear',
];

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
    !!(config.features?.linear && config.captain?.linearTeam && config.env?.LINEAR_API_KEY),
  ];

  const completed = done.filter(Boolean).length;
  if (completed >= SETUP_TOTAL) return null;

  const firstIncomplete = done.findIndex((d) => !d);
  return { completed, total: SETUP_TOTAL, currentStep: STEP_NAMES[firstIncomplete] ?? '' };
}

export function App(): React.ReactElement {
  const [activeTab, setActiveTab] = useState<Tab>('captain');
  const [showSettings, setShowSettings] = useState(false);
  const [projectFilter, setProjectFilter] = useState<string | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [createTaskOpen, setCreateTaskOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [detailItem, setDetailItem] = useState<TaskItem | null>(null);
  const [setupActive, setSetupActive] = useState(false);

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
      case 'nav-cron':
        setActiveTab('cron');
        break;
      case 'nav-analytics':
        setActiveTab('analytics');
        break;
      case 'act-settings':
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

  if (detailItem) {
    return (
      <div className="flex h-screen flex-col" style={{ background: 'var(--color-bg)' }}>
        <div className="h-8 shrink-0" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
        <div className="flex-1 overflow-hidden px-8 py-4">
          <TaskDetailView item={detailItem} onBack={() => setDetailItem(null)} />
        </div>
        <DevInfoBar />
        <CommandPalette
          open={paletteOpen}
          onClose={() => setPaletteOpen(false)}
          onAction={handlePaletteAction}
        />
        <CreateTaskModal open={createTaskOpen} onClose={() => setCreateTaskOpen(false)} />
        <ShortcutOverlay open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
        <ToastContainer />
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
          <SettingsPage onBack={() => setShowSettings(false)} />
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
              onClick={() => window.location.reload()}
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

        {/* Update banner */}
        <UpdateBanner />

        <div className="flex min-h-0 flex-1">
          {/* Sidebar */}
          <Sidebar
            activeTab={activeTab}
            onTabChange={setActiveTab}
            onNewTask={() => {
              setActiveTab('captain');
              setCreateTaskOpen(true);
            }}
            onOpenSettings={() => setShowSettings(true)}
            onToggleSetup={() => setSetupActive((v) => !v)}
            onDismissSetup={handleDismissSetup}
            projectFilter={projectFilter}
            onProjectFilter={setProjectFilter}
            setupProgress={setupProgress}
            setupActive={setupActive}
          />

          {/* Main content — always visible, popover floats above from sidebar */}
          <main
            className="flex-1 overflow-auto"
            style={{ background: 'var(--color-bg)', padding: '38px 32px 24px' }}
          >
            {activeTab === 'captain' && (
              <CaptainView
                projectFilter={projectFilter}
                onCreateTask={openCreateTask}
                onOpenDetail={setDetailItem}
              />
            )}
            {activeTab === 'scout' && <ScoutPage />}
            {activeTab === 'sessions' && <SessionsCard />}
            {activeTab === 'cron' && <CronJobsPanel variant="page" testId="cron-page" />}
            {activeTab === 'analytics' && <AnalyticsPage />}
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
    </div>
  );
}

function UpdateBanner(): React.ReactElement | null {
  const [updateReady, setUpdateReady] = useState(false);
  const [version, setVersion] = useState('');
  const [dismissed, setDismissed] = useState(false);

  useMountEffect(() => {
    if (!window.mandoAPI?.updates) return;
    window.mandoAPI.updates.onUpdateReady((info) => {
      setUpdateReady(true);
      setVersion(info.version);
    });
    return () => window.mandoAPI.updates.removeUpdateListeners();
  });

  if (!updateReady || dismissed) return null;

  return (
    <div
      className="flex shrink-0 items-center gap-3 px-4 py-1.5"
      style={{
        background: 'var(--color-surface-2)',
        borderBottom: '1px solid var(--color-border-subtle)',
      }}
    >
      <span className="text-[13px] font-medium" style={{ color: 'var(--color-text-1)' }}>
        Update available{' '}
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {version}
        </span>
      </span>
      <button
        onClick={() => window.mandoAPI.updates.installUpdate()}
        className="rounded-md px-2.5 py-0.5 text-[12px] font-semibold"
        style={{
          background: 'var(--color-accent)',
          color: 'var(--color-bg)',
          border: 'none',
          cursor: 'pointer',
        }}
      >
        Install & Restart
      </button>
      <button
        onClick={() => setDismissed(true)}
        className="ml-auto text-[13px] opacity-60 hover:opacity-100"
        style={{
          color: 'var(--color-text-3)',
          background: 'none',
          border: 'none',
          cursor: 'pointer',
        }}
      >
        ✕
      </button>
    </div>
  );
}
