import React, { useCallback, useRef } from 'react';
import { Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import { useGlobalKeyboard } from '#renderer/global/runtime/useKeyboardShortcuts';
import { useMainShortcuts, useNotificationClicks } from '#renderer/global/runtime/useNativeActions';
import { useTaskWorkbenchLookup } from '#renderer/global/runtime/useTaskCacheLookup';
import { useClaudeCodeVerification } from '#renderer/global/runtime/useClaudeCodeVerification';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import { DevInfoBar } from '#renderer/global/ui/DevInfoBar';
import { RootShellOverlays } from '#renderer/app/routes/RootShellOverlays';
import log from '#renderer/global/service/logger';
import { TAB_ROUTES } from '#renderer/global/service/routeHelpers';
import { router } from '#renderer/app/router';
import type { Tab } from '#renderer/app/Sidebar';

export function RootFrame(): React.ReactElement {
  const navigate = useNavigate();
  const paletteOpen = useUIStore((s) => s.paletteOpen);
  const createTaskOpen = useUIStore((s) => s.createTaskOpen);
  const shortcutsOpen = useUIStore((s) => s.shortcutsOpen);

  const showSettings = useRouterState({
    select: (s) => s.location.pathname.startsWith('/settings'),
  });

  useClaudeCodeVerification();

  const navigateTab = useCallback(
    (tab: Tab) => {
      void navigate({ to: TAB_ROUTES[tab] });
    },
    [navigate],
  );

  const openCreateTask = useCallback(() => {
    useUIStore.getState().openCreateTask();
  }, []);

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
    onGoBack: () => router.history.back(),
    onGoForward: () => router.history.forward(),
    onToggleSidebar: () => useUIStore.getState().toggleSidebar(),
  });

  // Main process shortcuts (Cmd+N from menu)
  useMainShortcuts((action: string) => {
    if (action === 'add-task') openCreateTask();
  });

  // Desktop notification click -> navigate to workbench
  const lookupWorkbench = useTaskWorkbenchLookup();
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const lookupRef = useRef(lookupWorkbench);
  lookupRef.current = lookupWorkbench;
  useNotificationClicks((data) => {
    if (data.item_id) {
      const id = Number(data.item_id);
      if (!Number.isNaN(id)) {
        const wbId = lookupRef.current(id);
        if (wbId) {
          void navigateRef.current({
            to: '/wb/$workbenchId',
            params: { workbenchId: String(wbId) },
          });
          return;
        }
        log.warn('notification click: no workbench for task', { taskId: id });
        void navigateRef.current({ to: '/' });
        return;
      }
    }
    if (data.kind.type === 'RateLimited') {
      void navigateRef.current({ to: '/' });
    }
  });

  // Command palette actions
  const handlePaletteAction = useCallback(
    (action: string) => {
      useUIStore.getState().closePalette();
      const navMap: Record<string, string> = {
        'nav-captain': '/',
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
      <RootShellOverlays onPaletteAction={handlePaletteAction} />
    </div>
  );
}
