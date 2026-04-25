import { useMemo, useRef, type Dispatch, type SetStateAction } from 'react';
import { useRouter } from '@tanstack/react-router';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import {
  useSidebarNav,
  useWorkbenchArchive,
  useWorkbenchUnarchive,
  useWorkbenchPin,
  useWorkbenchRename,
} from '#renderer/domains/captain';
import { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
import { featuresPatch } from '#renderer/global/service/configPatches';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useProjectWorkflows } from '#renderer/domains/captain/runtime/useProjectWorkflows';
import { TAB_ROUTES } from '#renderer/global/service/routeHelpers';
import type { SidebarActions, Tab } from '#renderer/global/runtime/SidebarContext';

interface SidebarActionsParams {
  projectFilter: string | null;
  setSetupActive: Dispatch<SetStateAction<boolean>>;
}

export function useSidebarActions({
  projectFilter,
  setSetupActive,
}: SidebarActionsParams): SidebarActions {
  const router = useRouter();
  const archiveMut = useWorkbenchArchive();
  const unarchiveMut = useWorkbenchUnarchive();
  const pinMut = useWorkbenchPin();
  const renameMut = useWorkbenchRename();
  const { save: saveConfig } = useConfigPatch();
  const { openInFinder } = useNativeActions().files;

  const { navigate, openTaskWorkbench, openWorktreeWorkbench, handleNewTerminal } = useSidebarNav();

  const archiveMutate = archiveMut.mutate;
  const unarchiveMutate = unarchiveMut.mutate;
  const pinMutate = pinMut.mutate;
  const renameMutate = renameMut.mutate;

  const pinPendingRef = useRef(false);
  pinPendingRef.current = pinMut.isPending;
  const renamePendingRef = useRef(false);
  renamePendingRef.current = renameMut.isPending;

  const { addProject, renameProject, removeProject } = useProjectWorkflows({
    navigate: (opts) => void navigate(opts),
    projectFilter,
  });

  return useMemo(
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
      unarchiveWorkbench: (id: number) => unarchiveMutate({ id }),
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
      router,
      archiveMutate,
      unarchiveMutate,
      pinMutate,
      renameMutate,
      saveConfig,
      addProject,
      renameProject,
      removeProject,
      openInFinder,
      setSetupActive,
    ],
  );
}
