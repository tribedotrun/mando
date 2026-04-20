import React, { useMemo, useRef, useState } from 'react';
import { useMatchRoute, useRouter, useRouterState } from '@tanstack/react-router';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import {
  useSidebarNav,
  useWorkbenchCtx,
  useWorkbenchArchive,
  useWorkbenchPin,
  useWorkbenchRename,
} from '#renderer/domains/captain';
import { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
import { featuresPatch } from '#renderer/global/service/configPatches';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useProjectWorkflows } from '#renderer/domains/captain/runtime/useProjectWorkflows';
import { useSetupProgress } from '#renderer/domains/captain/runtime/useSetupProgress';
import { TAB_ROUTES } from '#renderer/global/service/routeHelpers';
import {
  SidebarContext,
  type Tab,
  type SidebarActions,
  type SidebarState,
  type SidebarContextValue,
} from '#renderer/global/runtime/SidebarContext';

interface SidebarProviderProps {
  children: React.ReactNode;
}

export function SidebarProvider({ children }: SidebarProviderProps): React.ReactElement {
  const [setupActive, setSetupActive] = useState(false);
  const setupProgress = useSetupProgress();
  const router = useRouter();
  const archiveMut = useWorkbenchArchive();
  const pinMut = useWorkbenchPin();
  const renameMut = useWorkbenchRename();
  const { save: saveConfig } = useConfigPatch();
  const { openInFinder } = useNativeActions();

  const { navigate, openTaskWorkbench, openWorktreeWorkbench, handleNewTerminal } = useSidebarNav();

  const archiveMutate = archiveMut.mutate;
  const pinMutate = pinMut.mutate;
  const renameMutate = renameMut.mutate;

  const pinPendingRef = useRef(false);
  pinPendingRef.current = pinMut.isPending;
  const renamePendingRef = useRef(false);
  renamePendingRef.current = renameMut.isPending;

  const projectFilter = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });

  const { addProject, renameProject, removeProject } = useProjectWorkflows({
    navigate: (opts) => void navigate(opts),
    projectFilter,
  });

  const wbCtx = useWorkbenchCtx();
  const activeTaskId = wbCtx?.task?.id ?? null;
  const activeTerminalCwd = wbCtx?.worktreePath ?? null;

  const matchRoute = useMatchRoute();
  const activeTab: Tab = matchRoute({ to: '/scout', fuzzy: true })
    ? 'scout'
    : matchRoute({ to: '/sessions', fuzzy: true })
      ? 'sessions'
      : 'captain';

  const actions: SidebarActions = useMemo(
    () => ({
      changeTab: (tab: Tab) => {
        void navigate({ to: TAB_ROUTES[tab] });
      },
      openTask: (taskId: number, wbId?: number) => openTaskWorkbench(taskId, wbId),
      openTerminalSession: (session: { id?: number; project: string; cwd: string }) =>
        openWorktreeWorkbench(session.id, session.cwd),
      openSettings: () =>
        void navigate({ to: '/settings/$section', params: { section: 'general' } }),
      newTask: () => useUIStore.getState().openCreateTask(),
      newTerminal: (project: string) => void handleNewTerminal(project),
      goBack: () => router.history.back(),
      goForward: () => router.history.forward(),
      toggleSidebar: () => useUIStore.getState().toggleSidebar(),
      filterByProject: (project: string | null) => {
        void navigate({ to: '/', search: project ? { project } : {} });
      },

      archiveWorkbench: (id: number) => archiveMutate({ id }),
      pinWorkbench: (id: number) => {
        if (!pinPendingRef.current) pinMutate({ id, pinned: true });
      },
      unpinWorkbench: (id: number) => {
        if (!pinPendingRef.current) pinMutate({ id, pinned: false });
      },
      renameWorkbench: (id: number, title: string) => {
        if (!renamePendingRef.current) renameMutate({ id, title });
      },
      openWorkbenchInFinder: (worktree: string) => {
        openInFinder(worktree);
      },
      copyWorkbenchPath: (worktree: string) => {
        void copyToClipboard(worktree, 'Path copied');
      },

      addProject,
      renameProject,
      removeProject,

      toggleSetup: () => setSetupActive((v) => !v),
      dismissSetup: () => {
        setSetupActive(false);
        saveConfig(featuresPatch({ setupDismissed: true }));
      },
    }),
    [
      navigate,
      openTaskWorkbench,
      openWorktreeWorkbench,
      handleNewTerminal,
      archiveMutate,
      pinMutate,
      renameMutate,
      saveConfig,
      addProject,
      renameProject,
      removeProject,
      openInFinder,
    ],
  );

  const state: SidebarState = useMemo(
    () => ({
      activeTab,
      projectFilter,
      activeTerminalCwd,
      activeTaskId,
      setupProgress,
      setupActive,
    }),
    [activeTab, projectFilter, activeTerminalCwd, activeTaskId, setupProgress, setupActive],
  );

  const value: SidebarContextValue = useMemo(() => ({ state, actions }), [state, actions]);

  return <SidebarContext value={value}>{children}</SidebarContext>;
}
