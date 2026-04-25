import React, { useMemo, useState } from 'react';
import { useRouterState } from '@tanstack/react-router';
import { useWorkbenchCtx } from '#renderer/domains/captain';
import { useSidebarActions } from '#renderer/domains/captain/runtime/useSidebarActions';
import { useSetupProgress } from '#renderer/domains/captain/runtime/useSetupProgress';
import { useSidebarActiveTab } from '#renderer/domains/captain/runtime/useSidebarActiveTab';
import {
  SidebarContext,
  type SidebarContextValue,
  type SidebarState,
} from '#renderer/global/runtime/SidebarContext';

interface SidebarProviderProps {
  children: React.ReactNode;
}

export function SidebarProvider({ children }: SidebarProviderProps): React.ReactElement {
  const [setupActive, setSetupActive] = useState(false);
  const setupProgress = useSetupProgress();

  const projectFilter = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });

  const wbCtx = useWorkbenchCtx();
  const activeTaskId = wbCtx?.task?.id ?? null;
  const activeTerminalCwd = wbCtx?.worktreePath ?? null;

  const activeTab = useSidebarActiveTab();

  const actions = useSidebarActions({ projectFilter, setSetupActive });

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
