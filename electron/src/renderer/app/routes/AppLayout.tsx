import React, { useCallback, useState } from 'react';
import { Outlet } from '@tanstack/react-router';
import { usePanelRef } from 'react-resizable-panels';
import { useDataContext } from '#renderer/global/runtime/dataContext';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import log from '#renderer/global/service/logger';
import { usePanelLayout } from '#renderer/global/runtime/usePanelLayout';
import { Sidebar } from '#renderer/app/Sidebar';
import { RetryButton, SidebarProvider } from '#renderer/domains/captain/shell';
import { Button } from '#renderer/global/ui/primitives/button';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '#renderer/global/ui/primitives/resizable';
import { AppHeader } from '#renderer/app/AppHeader';
import { router } from '#renderer/app/router';
import { useUIStore } from '#renderer/global/runtime/useUIStore';

export function AppLayout(): React.ReactElement {
  const sidebarRef = usePanelRef();
  const { sseStatus, resetDataPlane } = useDataContext();
  const nativeActions = useNativeActions();
  const { restartDaemon } = nativeActions.app;
  const { openLogsFolder } = nativeActions.files;
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const { defaultLayout, onLayoutChanged } = usePanelLayout('sidebar-layout');

  // Toggle sidebar and explicitly sync collapsed state.
  const handleToggleSidebar = useCallback(() => {
    const panel = sidebarRef.current;
    if (!panel) return;
    if (panel.isCollapsed()) {
      panel.expand();
      setSidebarCollapsed(false);
    } else {
      panel.collapse();
      setSidebarCollapsed(true);
    }
  }, [sidebarRef]);

  const handleGoBack = useCallback(() => router.history.back(), []);
  const handleGoForward = useCallback(() => router.history.forward(), []);
  const handleNewTask = useCallback(() => useUIStore.getState().openCreateTask(), []);
  const handleRestartDaemon = useCallback(async () => {
    try {
      await restartDaemon();
      resetDataPlane();
    } catch (err) {
      log.warn('[AppLayout] restartDaemon failed', err);
    }
  }, [restartDaemon, resetDataPlane]);

  // Sync initial collapsed state (panel may restore collapsed from persistence)
  useMountEffect(() => {
    if (sidebarRef.current?.isCollapsed()) setSidebarCollapsed(true);
  });

  // Register the local toggle handler with the UI store. Sources outside
  // AppLayout (keyboard shortcut, sidebar context action) call
  // useUIStore.getState().toggleSidebar() to invoke this.
  useMountEffect(() => {
    useUIStore.getState().registerSidebarToggle(handleToggleSidebar);
    return () => useUIStore.getState().unregisterSidebarToggle();
  });

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
            onRetry={handleRestartDaemon}
          />
          <Button
            variant="link"
            size="xs"
            className="text-caption text-muted-foreground hover:text-foreground"
            onClick={openLogsFolder}
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
          panelRef={sidebarRef}
          id="sidebar"
          defaultSize="200px"
          minSize="160px"
          maxSize="400px"
          collapsible
          collapsedSize="0px"
          onResize={() => setSidebarCollapsed(sidebarRef.current?.isCollapsed() ?? false)}
        >
          <SidebarProvider>
            <Sidebar />
          </SidebarProvider>
        </ResizablePanel>

        <ResizableHandle />

        <ResizablePanel id="main" minSize="50%">
          <main className="flex h-full flex-col overflow-hidden bg-background">
            <AppHeader
              sidebarCollapsed={sidebarCollapsed}
              onToggleSidebar={handleToggleSidebar}
              onGoBack={handleGoBack}
              onGoForward={handleGoForward}
              onNewTask={handleNewTask}
            />
            <div className="relative z-0 min-h-0 flex-1">
              <Outlet />
            </div>
          </main>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
