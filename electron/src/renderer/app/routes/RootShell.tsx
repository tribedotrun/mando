import React, { useCallback, useRef } from 'react';
import { Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import { useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useGlobalKeyboard } from '#renderer/global/hooks/useKeyboardShortcuts';
import { queryKeys } from '#renderer/queryKeys';
import type { TaskListResponse, MandoConfig } from '#renderer/types';
import { useTaskActions } from '#renderer/domains/captain/hooks/useTaskActions';
import { useConfig } from '#renderer/hooks/queries';
import { useConfigSave } from '#renderer/hooks/mutations';
import { useUIStore } from '#renderer/app/uiStore';
import { DevInfoBar } from '#renderer/global/components/DevInfoBar';
import { CommandPalette } from '#renderer/global/components/CommandPalette';
import { CreateTaskModal } from '#renderer/domains/captain/components/AddTaskForm';
import { MergeModal } from '#renderer/domains/captain/components/MergeModal';
import { ShortcutOverlay } from '#renderer/global/components/ShortcutOverlay';
import log from '#renderer/logger';
import type { Tab } from '#renderer/app/Sidebar';

const MERGE_DISMISS_DELAY_MS = 1_200;

export function RootShell(): React.ReactElement {
  const navigate = useNavigate();
  const rqClient = useQueryClient();
  const actions = useTaskActions();
  const paletteOpen = useUIStore((s) => s.paletteOpen);
  const createTaskOpen = useUIStore((s) => s.createTaskOpen);
  const shortcutsOpen = useUIStore((s) => s.shortcutsOpen);
  const mergeItem = useUIStore((s) => s.mergeItem);

  const showSettings = useRouterState({
    select: (s) => s.location.pathname.startsWith('/settings'),
  });
  const currentProject = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });

  // Eager Claude Code check -- runs once when config is available
  useConfig(); // ensure config query is active for CC check below
  const saveMut = useConfigSave();
  const ccCheckDone = useRef(false);
  useMountEffect(() => {
    function tryCheck() {
      if (ccCheckDone.current) return;
      const cfg = rqClient.getQueryData<MandoConfig>(queryKeys.config.current());
      if (!cfg) return; // config not loaded yet
      if (cfg.features?.claudeCodeVerified || cfg.features?.setupDismissed) {
        ccCheckDone.current = true;
        return;
      }
      ccCheckDone.current = true;
      void window.mandoAPI
        ?.checkClaudeCode?.()
        .then((result) => {
          if (result.installed && result.works) {
            const current = rqClient.getQueryData<MandoConfig>(queryKeys.config.current());
            if (current && !current.features?.claudeCodeVerified) {
              const updated: MandoConfig = {
                ...current,
                features: { ...(current.features || {}), claudeCodeVerified: true },
              };
              saveMut.mutate(updated);
            }
          }
        })
        .catch((err) => log.warn('eager CC check failed:', err));
    }

    // Try immediately in case config is already cached
    tryCheck();

    // Subscribe to cache updates so we catch config arriving later
    const unsub = rqClient.getQueryCache().subscribe((event) => {
      if (event.query.queryKey[0] === 'config') tryCheck();
    });
    return unsub;
  });

  // Map tab names to routes
  const navigateTab = useCallback(
    (tab: Tab) => {
      const tabRoutes: Record<Tab, string> = {
        captain: '/captain',
        scout: '/scout',
        sessions: '/sessions',
      };
      void navigate({ to: tabRoutes[tab] });
    },
    [navigate],
  );

  const openCreateTask = useCallback(() => {
    void navigate({ to: '/captain' });
    useUIStore.getState().openCreateTask();
  }, [navigate]);

  // Global keyboard shortcuts
  useGlobalKeyboard({
    paletteOpen,
    shortcutsOpen,
    showSettings,
    modalOpen: createTaskOpen,
    onNavigate: navigateTab,
    onTogglePalette: useUIStore.getState().togglePalette,
    onOpenSettings: useCallback(() => {
      useUIStore.getState().closePalette();
      void navigate({ to: '/settings/$section', params: { section: 'general' } });
    }, [navigate]),
    onToggleShortcuts: useUIStore.getState().toggleShortcuts,
  });

  // Main process shortcuts (Cmd+N from menu)
  useMountEffect(() => {
    if (window.mandoAPI) {
      const handler = (action: string) => {
        if (action === 'add-task') openCreateTask();
      };
      window.mandoAPI.onShortcut(handler);
      return () => window.mandoAPI.removeShortcutListeners();
    }
  });

  // Desktop notification click → navigate to task detail
  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.onNotificationClick((data) => {
      if (data.item_id) {
        const id = Number(data.item_id);
        if (!Number.isNaN(id)) {
          const taskData = rqClient.getQueryData<TaskListResponse>(queryKeys.tasks.list());
          const task = taskData?.items.find((t) => t.id === id);
          if (task) {
            void navigate({ to: '/captain/tasks/$taskId', params: { taskId: String(id) } });
            return;
          }
        }
      }
      const kind = data.kind as { type: string } | undefined;
      if (kind?.type === 'RateLimited') {
        void navigate({ to: '/captain' });
      }
    });
    return () => window.mandoAPI.removeNotificationClickListeners();
  });

  // Command palette actions
  const handlePaletteAction = useCallback(
    (action: string) => {
      useUIStore.getState().closePalette();
      const navMap: Record<string, string> = {
        'nav-captain': '/captain',
        'nav-scout': '/scout',
        'recent-scout': '/scout',
        'nav-sessions': '/sessions',
      };
      if (navMap[action]) {
        void navigate({ to: navMap[action] });
      } else if (action === 'act-settings') {
        void navigate({ to: '/settings/$section', params: { section: 'general' } });
      } else if (action === 'act-create-task') {
        openCreateTask();
      }
    },
    [navigate, openCreateTask],
  );

  return (
    <div className="relative flex h-screen flex-col bg-background">
      {/* Route content */}
      <Outlet />

      <DevInfoBar />

      {/* Global overlays */}
      {mergeItem && (
        <MergeModal
          item={mergeItem}
          onConfirm={(itemId, pr, project) => {
            void actions.handleMerge(itemId, pr, project).then(() => {
              setTimeout(() => useUIStore.getState().setMergeItem(null), MERGE_DISMISS_DELAY_MS);
            });
          }}
          onCancel={() => useUIStore.getState().setMergeItem(null)}
          pending={actions.mergePending}
          result={actions.mergeResult}
        />
      )}
      <CommandPalette
        open={paletteOpen}
        onClose={() => useUIStore.getState().closePalette()}
        onAction={handlePaletteAction}
      />
      <CreateTaskModal
        open={createTaskOpen}
        onClose={() => useUIStore.getState().closeCreateTask()}
        initialProject={currentProject}
      />
      <ShortcutOverlay
        open={shortcutsOpen}
        onClose={() => useUIStore.getState().closeShortcuts()}
      />
    </div>
  );
}
