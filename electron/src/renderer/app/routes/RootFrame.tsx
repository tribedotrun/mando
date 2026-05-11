import React, { useCallback, useRef } from 'react';
import { Outlet, useNavigate, useRouterState } from '@tanstack/react-router';
import { useGlobalKeyboard } from '#renderer/global/runtime/useKeyboardShortcuts';
import { useMainShortcuts } from '#renderer/global/runtime/useNativeActions';
import { useNotificationClickRouter } from '#renderer/global/runtime/useNotificationClickRouter';
import { useClaudeCodeVerification } from '#renderer/global/runtime/useClaudeCodeVerification';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import { DevInfoBar } from '#renderer/global/ui/DevInfoBar';
import { RootShellOverlays } from '#renderer/app/routes/RootShellOverlays';
import { TAB_ROUTES } from '#renderer/global/service/routeHelpers';
import { router } from '#renderer/app/router';
import type { Tab } from '#renderer/app/Sidebar';

export function RootFrame(): React.ReactElement {
  const navigate = useNavigate();
  const paletteOpen = useUIStore((s) => s.paletteOpen);
  const shortcutsOpen = useUIStore((s) => s.shortcutsOpen);

  const showSettings = useRouterState({
    select: (s) => s.location.pathname.startsWith('/settings'),
  });
  const currentProject = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
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

  // The home navigator lets non-React callers (zustand store, keyboard shortcuts
  // routed through the store) bring the user back to / so the inline composer
  // can take focus. The current route's `?project=` is forwarded so users
  // coming from project-scoped views (workbench, transcript, project-filtered
  // home) keep the project pre-selected in the composer.
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const currentProjectRef = useRef(currentProject);
  currentProjectRef.current = currentProject;
  useMountEffect(() => {
    useUIStore.getState().registerHomeNavigator(() => {
      const project = currentProjectRef.current;
      void navigateRef.current({ to: '/', search: project ? { project } : {} });
    });
    return () => useUIStore.getState().unregisterHomeNavigator();
  });

  // Global keyboard shortcuts
  useGlobalKeyboard({
    paletteOpen,
    shortcutsOpen,
    showSettings,
    modalOpen: false,
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

  // Desktop notification click -> route by NotificationKind (task→workbench, scout→reader)
  useNotificationClickRouter();

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
